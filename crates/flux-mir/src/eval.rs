//! Constant evaluation: running a constant's MIR at compile time.
//!
//! A constant's value is computed by interpreting the MIR of its
//! initializer, and of any `const fn` it calls, so every expression form
//! works the same at compile time as at run time. Unlike run time,
//! arithmetic that overflows and division by zero are errors (E0080),
//! with rustc's messages: "attempt to compute `i32::MAX + 1_i32`, which
//! would overflow". An error inside a call is reported at the call in the
//! constant.
//!
//! An enum's discriminants written as expressions are anonymous
//! constants, evaluated as they are needed, as rustc does: a unit
//! variant's path cast to an integer needs that variant's only, while
//! making a value of the enum (which needs its layout) or a body that
//! matches on it needs them all, so a discriminant that needs those of
//! its own enum is a cycle (E0391).

use std::rc::Rc;
use std::sync::{Arc, LazyLock};

use rustc_hash::{FxHashMap, FxHashSet};

use flux_arena::ArenaMap;
use flux_hir::{ConstId, FunctionId, ItemTree};
use flux_syntax::TextRange;
use flux_ty::{ConstInstance, GenericArgs, IntTy, Signatures, Ty, TyKind, subst_list};

use crate::{
    AggregateKind, AssertMessage, BinOp, Body, Constant, Operand, Place, ProjectionElem, Rvalue,
    StatementKind, TerminatorKind, UnOp,
};

/// A value computed at compile time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    /// Two's complement bits, truncated to the type's width.
    Int {
        bits: u128,
        ty: IntTy,
    },
    Bool(bool),
    /// A thin raw address without an allocation or reference validity. Kept
    /// distinct from integers so const casts cannot recover its numeric value.
    RawAddr(u64),
    /// A tuple or struct, fields in order; `()` has none.
    Aggregate(Arc<[Value]>),
    /// Variant `index` of enum `strukt`, its fields in order.
    Variant {
        strukt: flux_hir::StructId,
        index: u32,
        fields: Arc<[Value]>,
    },
    /// The address of a static, which reading through gives its
    /// initial value.
    Static(ConstId),
    /// A reference/raw pointer into the original static, not a promoted copy.
    StaticPart {
        id: ConstId,
        path: Arc<[StaticProjection]>,
    },
    /// A shared reference to a value: what it points to, which cannot
    /// change while it is borrowed. A reference to a slice holds the
    /// elements as an aggregate.
    Ref(Arc<Value>),
    /// A transient borrow of interior-mutable storage. Casts must retain
    /// this origin even if the pointer's final pointee type hides it.
    InteriorRef(Arc<Value>),
    /// A pointer cast changes the accessed type, not its backing allocation.
    /// Retain the original pointer type for layout and emitted storage.
    PointerView {
        pointer: Arc<Value>,
        source: Ty,
        target: Ty,
    },
    /// Named frame storage is not promoted into a constant allocation. Raw
    /// access to it needs an addressable frame-memory model, not a value copy.
    LocalRef {
        value: Arc<Value>,
        origin: LocalOrigin,
        raw: bool,
    },
}

/// Constant projections contain no frame-local indices or dereferences.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticProjection {
    Field(u32),
    Index(u64),
    Downcast(u32),
    Subslice { from: u64, len: u64 },
}

/// The stored type at an address path, before any raw pointer reinterpretation.
pub fn static_path_type(sigs: &Signatures, mut ty: Ty, path: &[StaticProjection]) -> Option<Ty> {
    let mut variant = 0;
    for p in path {
        match *p {
            StaticProjection::Downcast(v) => variant = v,
            StaticProjection::Field(i) => {
                ty = *sigs.variant_fields(ty, variant)?.get(i as usize)?;
                variant = 0;
            }
            StaticProjection::Index(_) => ty = ty.sequence_elem()?,
            StaticProjection::Subslice { len, .. } => ty = Ty::array(ty.sequence_elem()?, len),
        }
    }
    Some(ty)
}

impl Value {
    /// Unit and fieldless aggregates share one empty allocation.
    pub fn unit() -> Self {
        Self::Aggregate(Self::empty_fields())
    }

    fn empty_fields() -> Arc<[Value]> {
        static EMPTY: LazyLock<Arc<[Value]>> = LazyLock::new(Arc::default);
        Arc::clone(&EMPTY)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalError {
    pub range: TextRange,
    pub message: String,
    /// The error is another constant's, which reports it itself.
    pub nested: bool,
    /// A cycle back to this discriminant's constant, which reports it
    /// once, at its value, as rustc does.
    pub cycle: Option<ConstId>,
}

/// The values of constants: of those without generic parameters by
/// item, and of the instances of others that code without generic
/// parameters names.
#[derive(Debug, Default)]
pub struct ConstValues {
    pub items: ArenaMap<flux_hir::Const, Value>,
    pub instances: FxHashMap<ConstInstance, Value>,
}

impl ConstValues {
    /// The value of constant `id` with generic arguments `args`, if it
    /// was evaluated.
    pub fn get(&self, id: ConstId, args: GenericArgs) -> Option<&Value> {
        if args.is_empty() {
            self.items.get(id)
        } else {
            self.instances.get(&ConstInstance { konst: id, args })
        }
    }
}

/// How deep constants may need others: rustc's query depth limit.
const CONST_DEPTH_LIMIT: usize = 128;

/// How many statements a constant may run before evaluation gives up;
/// rustc allows loops of millions of iterations.
const STEP_LIMIT: u64 = 50_000_000;
/// How deep calls may nest: rustc allows 128 frames, the constant's own
/// included.
const DEPTH_LIMIT: u32 = 127;
/// The most elements a repeat expression may make, each a value.
const MAX_ELEMENTS: u64 = 1 << 24;

/// Evaluates constants on demand, each instance once.
pub struct Evaluator<'a> {
    pub items: &'a ItemTree,
    pub sigs: &'a Signatures,
    pub functions: &'a ArenaMap<flux_hir::Function, Body>,
    pub consts: &'a ArenaMap<flux_hir::Const, Body>,
    /// Each constant's name, for messages.
    pub names: &'a dyn Fn(ConstId) -> String,
    values: FxHashMap<ConstInstance, Result<Value, EvalError>>,
    in_progress: Vec<ConstInstance>,
    /// The promoted constants of each body being run, innermost last.
    promoted: Vec<&'a [Value]>,
    /// Cleared frame storage, reused after a call returns (including errors).
    frames: Vec<Vec<Option<Value>>>,
    arguments: Vec<Vec<Value>>,
    local_borrows: Vec<Vec<Option<LocalOrigin>>>,
    borrow_frames: Vec<Vec<Option<LocalOrigin>>>,
    live_borrows: FxHashSet<u64>,
    next_borrow: u64,
    view_nodes: u64,
    view_layouts: FxHashMap<Ty, Rc<crate::layout::Layout>>,
    view_fields: FxHashMap<(Ty, u32), Rc<[MemoryField]>>,
    promotion_flags: FxHashMap<usize, Rc<[u8]>>,
    /// Every discriminant of the enums whose values were made or matched
    /// on, as bits.
    discriminants: FxHashMap<flux_hir::StructId, Result<Box<[u128]>, EvalError>>,
    /// The functions whose matches' enums have their discriminants.
    switched: FxHashSet<FunctionId>,
    /// The constants whose own value failed, in the order they did,
    /// with why: what [`Self::eval_instance`]'s callers report.
    pub failures: Vec<(ConstInstance, EvalError)>,
    /// The constants asked for that had no MIR, in order.
    pub unbuilt: Vec<ConstId>,
    steps: u64,
}

impl<'a> Evaluator<'a> {
    pub fn new(
        items: &'a ItemTree,
        sigs: &'a Signatures,
        functions: &'a ArenaMap<flux_hir::Function, Body>,
        consts: &'a ArenaMap<flux_hir::Const, Body>,
        names: &'a dyn Fn(ConstId) -> String,
    ) -> Evaluator<'a> {
        Evaluator {
            items,
            sigs,
            functions,
            consts,
            names,
            values: FxHashMap::default(),
            in_progress: Vec::new(),
            discriminants: FxHashMap::default(),
            switched: FxHashSet::default(),
            failures: Vec::new(),
            unbuilt: Vec::new(),
            steps: 0,
            promoted: Vec::new(),
            frames: Vec::new(),
            arguments: Vec::new(),
            local_borrows: Vec::new(),
            borrow_frames: Vec::new(),
            live_borrows: FxHashSet::default(),
            next_borrow: 1,
            view_nodes: 0,
            view_layouts: FxHashMap::default(),
            view_fields: FxHashMap::default(),
            promotion_flags: FxHashMap::default(),
        }
    }

    /// The value of constant `id`, which has no generic parameters,
    /// evaluated if it was not already.
    pub fn eval_const(&mut self, id: ConstId) -> Result<Value, EvalError> {
        self.eval_instance(ConstInstance {
            konst: id,
            args: &[],
        })
    }

    /// The value of constant `c` with its generic arguments, which must
    /// be known types: a trait's for them is its `impl` block's, or the
    /// trait's default.
    pub fn eval_instance(&mut self, c: ConstInstance) -> Result<Value, EvalError> {
        let c = flux_ty::traits::resolve_const(self.items, self.sigs, c);
        if let Some(result) = self.values.get(&c) {
            return result.clone();
        }
        let Some(body) = self.consts.get(c.konst) else {
            self.unbuilt.push(c.konst);
            return Err(error(
                TextRange::default(),
                "constant without a value".to_owned(),
            ));
        };
        if self.in_progress.contains(&c) {
            let data = &self.items.consts[c.konst];
            if data.discriminant.is_some() {
                return Err(EvalError {
                    range: data.def_range,
                    message: format!(
                        "cycle detected when simplifying constant for the type system `{}`",
                        (self.names)(c.konst)
                    ),
                    nested: false,
                    cycle: Some(c.konst),
                });
            }
            let message = format!(
                "cycle detected when const-evaluating `{}`",
                (self.names)(c.konst)
            );
            return Err(error(TextRange::default(), message));
        }
        // `const A: u32 = W::<W<T>>::A;` needs ever more instances.
        if self.in_progress.len() >= CONST_DEPTH_LIMIT {
            return Err(error(
                TextRange::default(),
                "queries overflow the depth limit!".to_owned(),
            ));
        }
        self.in_progress.push(c);
        let steps = self.steps;
        let view_nodes = self.view_nodes;
        self.steps = 0;
        self.view_nodes = 0;
        // A constant's own arithmetic is always checked.
        let result = self
            .switched(body)
            .and_then(|()| self.run(body, Vec::new(), c.args, 0, true))
            .and_then(|value| {
                self.check_escaping_borrows(
                    &value,
                    self.sigs.consts[c.konst].subst(c.args),
                    self.items.consts[c.konst].def_range,
                )?;
                Ok(value)
            });
        self.steps = steps;
        self.view_nodes = view_nodes;
        self.in_progress.pop();
        if let Err(e) = &result
            && !e.nested
            && e.cycle.is_none_or(|k| k == c.konst)
        {
            self.failures.push((c, e.clone()));
        }
        self.values.insert(c, result.clone());
        result
    }

    fn inline_interior(&self, value: &Value, ty: Ty) -> bool {
        self.inline_lang(value, ty, &["cell", "ref_cell", "unsafe_cell"])
    }

    fn inline_lang(&self, value: &Value, ty: Ty, langs: &[&str]) -> bool {
        let ty = flux_ty::traits::normalize(self.items, self.sigs, &[], ty);
        if let TyKind::Adt(adt, _) = ty.kind()
            && langs
                .iter()
                .any(|lang| flux_ty::is_lang_struct(self.items, adt.strukt, lang))
        {
            return true;
        }
        match value {
            Value::Aggregate(fields) => {
                if let TyKind::Array(elem, _) | TyKind::Slice(elem) = ty.kind() {
                    fields.iter().any(|v| self.inline_lang(v, elem, langs))
                } else {
                    self.sigs.fields(ty).is_some_and(|types| {
                        fields
                            .iter()
                            .zip(types)
                            .any(|(v, &t)| self.inline_lang(v, t, langs))
                    })
                }
            }
            Value::Variant { index, fields, .. } => {
                self.sigs.variant_fields(ty, *index).is_some_and(|types| {
                    fields
                        .iter()
                        .zip(types)
                        .any(|(v, &t)| self.inline_lang(v, t, langs))
                })
            }
            _ => false,
        }
    }

    fn check_escaping_borrows(
        &mut self,
        value: &Value,
        ty: Ty,
        range: TextRange,
    ) -> Result<(), EvalError> {
        let ty = flux_ty::traits::normalize(self.items, self.sigs, &[], ty);
        match value {
            Value::LocalRef { .. } => return Err(error(range,
                "reference to local storage cannot escape constant evaluation".into())),

            Value::PointerView { pointer, source, target } => {
                self.check_escaping_borrows(pointer, *source, range)?;
                if let TyKind::Ref(_, pointee) = ty.kind() {
                    // A cast cannot introduce mutable storage into an existing
                    // allocation. Ordinary static Cell field borrows retain
                    // their original type and do not need a PointerView.
                    if flux_ty::traits::has_interior_mutability(self.items, self.sigs, pointee) {
                        return Err(error(range, "references to reinterpreted interior-mutable storage in constants are not supported yet".into()));
                    }
                    self.pointer_value(pointer, *source, *target, range)?;
                }
            }
            Value::InteriorRef(_) => return Err(error(range,
                "shared borrow of a temporary with interior mutability cannot escape constant evaluation".into())),
            Value::Ref(inner) => {
                let target = match ty.kind() {
                    TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t,
                    _ => Ty::ERROR,
                };
                if matches!(ty.kind(), TyKind::Ref(..)) && self.inline_interior(inner, target) {
                    return Err(error(range, "shared borrow of a temporary with interior mutability cannot escape constant evaluation".into()));
                }
                self.check_escaping_borrows(inner, target, range)?;
            }
            Value::Aggregate(fields) => {
                let element = match ty.kind() {
                    TyKind::Array(elem, _) | TyKind::Slice(elem) => Some(elem),
                    _ => None,
                };
                let types = self.sigs.fields(ty);
                for (i, field) in fields.iter().enumerate() {
                    // A raw-pointer cast can erase the original aggregate
                    // type. Its stored borrow origins must still be checked.
                    let ty = element
                        .or_else(|| types.and_then(|types| types.get(i).copied()))
                        .unwrap_or(Ty::ERROR);
                    self.check_escaping_borrows(field, ty, range)?;
                }
            }
            Value::Variant { index, fields, .. } => {
                let types = self.sigs.variant_fields(ty, *index);
                for (i, field) in fields.iter().enumerate() {
                    let ty = types
                        .and_then(|types| types.get(i).copied())
                        .unwrap_or(Ty::ERROR);
                    self.check_escaping_borrows(field, ty, range)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Read the value a cast pointer views, without changing its allocation.
    fn pointer_value(
        &mut self,
        pointer: &Value,
        source: Ty,
        target: Ty,
        range: TextRange,
    ) -> Result<Value, EvalError> {
        let value = match pointer {
            Value::Ref(value) | Value::InteriorRef(value) => (**value).clone(),
            Value::LocalRef { value, origin, raw } => {
                if !self.live_borrows.contains(&origin.token) {
                    return Err(error(
                        range,
                        "dangling reference to local constant storage".into(),
                    ));
                }
                if *raw && origin.mutable {
                    return Err(error(range, "raw pointer reads of mutable local storage in constants are not supported yet".into()));
                }
                if source == Ty::ERROR {
                    return Ok((**value).clone());
                }
                (**value).clone()
            }
            Value::Static(_) | Value::StaticPart { .. } => {
                let locals = [Some(pointer.clone())];
                let place = Place {
                    local: crate::Local::from_raw(0),
                    projection: vec![ProjectionElem::Deref].into(),
                };
                self.operand(&locals, &[], &Operand::Copy(place), range)?
            }
            _ => return Err(unsupported_view(range)),
        };
        let mut from = pointer_pointee(source).ok_or_else(|| unsupported_view(range))?;
        let mut to = pointer_pointee(target).ok_or_else(|| unsupported_view(range))?;
        // A wide pointer retains its original element-count metadata. The
        // structural value supplies that count, not an inferred byte length.
        if let Value::Aggregate(fields) = &value {
            let len = fields.len() as u64;
            from = match from.kind() {
                TyKind::Slice(elem) => Ty::array(elem, len),
                TyKind::Str => Ty::array(Ty::int(IntTy::U8), len),
                _ => from,
            };
            to = match to.kind() {
                TyKind::Slice(elem) => Ty::array(elem, len),
                TyKind::Str => Ty::array(Ty::int(IntTy::U8), len),
                _ => to,
            };
        }
        let stored = self.view_layout(from, range)?;
        let viewed = self.view_layout(to, range)?;
        if !stored.exact || !viewed.exact || stored.align < viewed.align {
            return Err(unsupported_view(range));
        }
        let result = self.memory_value(&value, from, 0, to, range, 0)?;
        if matches!(pointer_pointee(target).map(Ty::kind), Some(TyKind::Str))
            && str_value(&Value::Ref(Arc::new(result.clone()))).is_none()
        {
            return Err(error(range, "invalid utf-8 in constant reference".into()));
        }
        Ok(result)
    }

    /// Interpret initialized typed storage at a byte offset. Pointer values
    /// remain symbolic: their addresses are never converted to integer bits.
    fn memory_value(
        &mut self,
        value: &Value,
        from: Ty,
        offset: u64,
        to: Ty,
        range: TextRange,
        depth: u32,
    ) -> Result<Value, EvalError> {
        self.steps += 1;
        if self.steps > STEP_LIMIT {
            return Err(error(
                range,
                "constant evaluation is taking a long time".into(),
            ));
        }
        if depth > DEPTH_LIMIT {
            return Err(unsupported_view(range));
        }
        let from = flux_ty::traits::normalize(self.items, self.sigs, &[], from);
        let to = flux_ty::traits::normalize(self.items, self.sigs, &[], to);
        let stored = self.view_layout(from, range)?;
        let viewed = self.view_layout(to, range)?;
        if !stored.exact || !viewed.exact {
            return Err(unsupported_view(range));
        }
        if viewed.uninhabited {
            return Err(error(range, "invalid uninhabited constant value".into()));
        }
        let end = offset
            .checked_add(viewed.size)
            .filter(|&end| end <= stored.size)
            .ok_or_else(|| error(range, "constant pointer read is out of bounds".into()))?;
        if offset == 0 && from == to {
            return Ok(value.clone());
        }
        if offset == 0 && from.is_pointer() && to.is_pointer() {
            if matches!(to.kind(), TyKind::Ref(..)) {
                // An unbacked raw address cannot establish reference validity.
                if matches!(value, Value::RawAddr(_)) {
                    return Err(unsupported_view(range));
                }
                if matches!(to.kind(), TyKind::Ref(flux_ty::Mutability::Mut, _)) {
                    return Err(unsupported_view(range));
                }
            }
            return Ok(pointer_view(value.clone(), from, to));
        }
        let bits = match value {
            Value::Int { bits, .. } if from.scalar_int().is_some() => Some(*bits),
            Value::Bool(b) if from == Ty::BOOL => Some(u128::from(*b)),
            Value::RawAddr(address) if from.is_pointer() => Some(u128::from(*address)),
            _ => None,
        };
        if let Some(bits) = bits
            && offset < 16
        {
            let bits = bits >> (offset * 8);
            if matches!(to.kind(), TyKind::RawPtr(_, pointee) if !pointee.is_unsized()) {
                return Ok(Value::RawAddr(truncate(bits, IntTy::Usize) as u64));
            }
            if let Some(int) = to.scalar_int() {
                let bits = truncate(bits, int);
                if to == Ty::CHAR && char::from_u32(bits as u32).is_none() {
                    return Err(error(range, "invalid char in constant value".into()));
                }
                return Ok(Value::Int { bits, ty: int });
            }
            if to == Ty::BOOL {
                return match bits & 0xff {
                    0 => Ok(Value::Bool(false)),
                    1 => Ok(Value::Bool(true)),
                    _ => Err(error(range, "invalid bool in constant value".into())),
                };
            }
        }
        // Descend into a source field only when the complete read lies in
        // that field. Padding and reads spanning unrelated fields are not
        // synthesized as initialized bytes.
        match value {
            Value::Aggregate(fields) if matches!(from.kind(), TyKind::Array(..)) => {
                let elem = from.sequence_elem().unwrap();
                let stride = self.view_layout(elem, range)?.size;
                if let Some(index) = offset.checked_div(stride) {
                    let within = offset % stride;
                    if viewed.size <= stride - within
                        && let Some(field) = fields.get(index as usize)
                    {
                        return self.memory_value(field, elem, within, to, range, depth + 1);
                    }
                }
            }
            Value::Aggregate(fields) | Value::Variant { fields, .. } => {
                let variant = match value {
                    Value::Variant { index, .. } => *index,
                    _ => 0,
                };
                let index = self.memory_fields(from, variant, range)?;
                let next = index.partition_point(|field| field.at <= offset);
                if let Some(field) = next.checked_sub(1).and_then(|i| index.get(i))
                    && end <= field.end
                    && let Some(value) = fields.get(field.index as usize)
                {
                    return self.memory_value(
                        value,
                        field.ty,
                        offset - field.at,
                        to,
                        range,
                        depth + 1,
                    );
                }
            }
            _ => {}
        }
        match to.kind() {
            TyKind::Array(elem, len) => {
                let len = len
                    .as_usize()
                    .filter(|&len| len <= MAX_ELEMENTS)
                    .ok_or_else(|| unsupported_view(range))?;
                let stride = self.view_layout(elem, range)?.size;
                self.charge_view(len, range)?;
                let mut fields = Vec::with_capacity(len as usize);
                for i in 0..len {
                    fields.push(self.memory_value(
                        value,
                        from,
                        offset + stride * i,
                        elem,
                        range,
                        depth + 1,
                    )?);
                }
                Ok(Value::Aggregate(fields.into()))
            }
            TyKind::Tuple(_) | TyKind::Adt(..) => {
                let types = self
                    .sigs
                    .fields(to)
                    .ok_or_else(|| unsupported_view(range))?;
                if matches!(to.kind(), TyKind::Adt(adt, _) if self.sigs.variants(to).is_some() || self.sigs.unions.contains(&adt.strukt))
                {
                    return Err(unsupported_view(range));
                }
                self.charge_view(types.len() as u64, range)?;
                let mut fields = Vec::with_capacity(types.len());
                for (i, &ty) in types.iter().enumerate() {
                    fields.push(self.memory_value(
                        value,
                        from,
                        offset + viewed.field_offset(0, i as u32),
                        ty,
                        range,
                        depth + 1,
                    )?);
                }
                self.validate_lang_value(to, &fields, range)?;
                Ok(Value::Aggregate(fields.into()))
            }
            _ => Err(unsupported_view(range)),
        }
    }

    fn charge_view(&mut self, fields: u64, range: TextRange) -> Result<(), EvalError> {
        self.view_nodes = self
            .view_nodes
            .checked_add(fields)
            .filter(|&nodes| nodes <= MAX_ELEMENTS)
            .ok_or_else(|| {
                error(
                    range,
                    "constant pointer view has too many fields to evaluate".into(),
                )
            })?;
        Ok(())
    }

    fn view_layout(
        &mut self,
        ty: Ty,
        range: TextRange,
    ) -> Result<Rc<crate::layout::Layout>, EvalError> {
        if let Some(layout) = self.view_layouts.get(&ty) {
            return Ok(layout.clone());
        }
        let layout = Rc::new(self.layout_of(ty, range)?);
        self.view_layouts.insert(ty, layout.clone());
        Ok(layout)
    }

    fn memory_fields(
        &mut self,
        ty: Ty,
        variant: u32,
        range: TextRange,
    ) -> Result<Rc<[MemoryField]>, EvalError> {
        if let Some(index) = self.view_fields.get(&(ty, variant)) {
            return Ok(index.clone());
        }
        let layout = self.view_layout(ty, range)?;
        let types = self.sigs.variant_fields(ty, variant).unwrap_or_default();
        let mut fields = Vec::with_capacity(types.len());
        for (i, &ty) in types.iter().enumerate() {
            let at = layout.field_offset(variant, i as u32);
            let size = self.view_layout(ty, range)?.size;
            if size != 0 {
                let end = at
                    .checked_add(size)
                    .ok_or_else(|| unsupported_view(range))?;
                fields.push(MemoryField {
                    at,
                    end,
                    index: i as u32,
                    ty,
                });
            }
        }
        fields.sort_unstable_by_key(|field| field.at);
        let fields: Rc<[_]> = fields.into();
        self.view_fields.insert((ty, variant), fields.clone());
        Ok(fields)
    }

    fn validate_lang_value(
        &mut self,
        ty: Ty,
        fields: &[Value],
        range: TextRange,
    ) -> Result<(), EvalError> {
        let TyKind::Adt(adt, _) = ty.kind() else {
            return Ok(());
        };
        let lang = |name| flux_ty::is_lang_struct(self.items, adt.strukt, name);
        if lang("non_null") && matches!(fields.first(), Some(Value::RawAddr(0))) {
            return Err(error(range, "null pointer in `NonNull` constant".into()));
        }
        if lang("nonzero_u32") && matches!(fields.first(), Some(Value::Int { bits: 0, .. })) {
            return Err(error(range, "zero in `NonZeroU32` constant".into()));
        }
        if lang("alignment") {
            let Some(Value::Int { bits, .. }) = fields.first() else {
                return Err(unsupported_view(range));
            };
            if !bits.is_power_of_two() || *bits > (1u128 << 63) {
                return Err(error(range, "invalid alignment in constant value".into()));
            }
        }
        if let Some(pointee) = flux_ty::box_pointee(ty) {
            let layout = self.view_layout(pointee, range)?;
            // There is no const heap allocator here. A nonempty Box cannot
            // acquire ownership of promoted/read-only data by changing types.
            if !layout.exact || layout.size != 0 {
                return Err(unsupported_view(range));
            }
            let mut pointer = fields.first().ok_or_else(|| unsupported_view(range))?;
            while let Value::Aggregate(fields) = pointer {
                pointer = fields.first().ok_or_else(|| unsupported_view(range))?;
            }
            match pointer {
                Value::RawAddr(address) if *address != 0 && address % layout.align == 0 => {}
                Value::Ref(_) | Value::Static(_) | Value::StaticPart { .. } => {}
                Value::PointerView { source, .. } => {
                    let stored = pointer_pointee(*source).ok_or_else(|| unsupported_view(range))?;
                    let align = match stored.kind() {
                        TyKind::Slice(elem) => self.view_layout(elem, range)?.align,
                        TyKind::Str => 1,
                        _ => self.view_layout(stored, range)?.align,
                    };
                    if align < layout.align {
                        return Err(unsupported_view(range));
                    }
                }
                _ => return Err(unsupported_view(range)),
            }
        }
        Ok(())
    }

    /// Read-only expressions may be promoted; runtime inputs and mutable
    /// places must retain frame identity. The linear worklist also handles
    /// temporaries whose definitions are in later-numbered basic blocks.
    fn temporary_flags(&mut self, body: &Body) -> Rc<[u8]> {
        const PURE: u8 = 1;
        const EXTENDED: u8 = 2;
        const WRITTEN: u8 = 4;
        let key = std::ptr::from_ref(body) as usize;
        if let Some(flags) = self.promotion_flags.get(&key) {
            return flags.clone();
        }
        let n = body.locals.len();
        let mut flags = vec![0u8; n];
        let mut definitions = vec![None; n];
        let mut counts = vec![0u8; n];
        let mut type_calls = vec![false; n];
        let extended = body.owner.is_none() && body.upvars.is_none();
        if extended {
            for (id, decl) in body.locals.iter() {
                if decl.kind.is_temp() {
                    flags[id.index()] |= EXTENDED;
                }
            }
        }
        for block in body.blocks.values() {
            for statement in &block.statements {
                match &statement.kind {
                    StatementKind::Assign(place, value) => {
                        let i = place.local.index();
                        counts[i] = counts[i].saturating_add(1);
                        if place.projection.is_empty() {
                            definitions[i] = Some(value);
                        } else {
                            flags[i] |= WRITTEN;
                        }
                    }
                    StatementKind::StorageDead(local) => flags[local.index()] &= !EXTENDED,
                    _ => {}
                }
            }
            if let TerminatorKind::Call { func, dest, .. } = &block.terminator.kind {
                let i = dest.local.index();
                counts[i] = counts[i].saturating_add(1);
                type_calls[i] = dest.projection.is_empty() && self.is_type_intrinsic(*func);
            }
        }
        let mut pending = vec![0usize; n];
        let mut uses = vec![Vec::new(); n];
        let mut ready = Vec::new();
        for (local, decl) in body.locals.iter() {
            let i = local.index();
            if counts[i] > 1 {
                flags[i] |= WRITTEN;
            }
            if counts[i] != 1
                || flags[i] & WRITTEN != 0
                || !(decl.kind.is_temp() || matches!(decl.kind, crate::LocalKind::Static(_)))
            {
                continue;
            }
            let mut deps = Vec::new();
            let mut add_place = |place: &Place, deref: bool| {
                if !deref && place.projection.contains(&ProjectionElem::Deref) {
                    return false;
                }
                deps.push(place.local);
                for elem in place.projection.iter() {
                    if let ProjectionElem::Index(local) = elem {
                        deps.push(*local);
                    }
                }
                true
            };
            let mut add_operand = |operand: &Operand| match operand {
                Operand::Const(_) => true,
                Operand::Copy(place) | Operand::Move(place) => add_place(place, false),
            };
            let eligible = if type_calls[i] {
                true
            } else {
                match definitions[i] {
                    Some(
                        Rvalue::Use(op)
                        | Rvalue::UnaryOp(_, op)
                        | Rvalue::Cast(op, _)
                        | Rvalue::Unsize(op, _),
                    ) => add_operand(op),
                    Some(Rvalue::BinaryOp(_, a, b)) => add_operand(a) && add_operand(b),
                    Some(Rvalue::Aggregate(_, ops)) => ops.iter().all(&mut add_operand),
                    Some(Rvalue::Repeat(op, _)) => add_operand(op),
                    Some(Rvalue::Ref {
                        mutability: flux_ty::Mutability::Not,
                        place,
                        ..
                    }) => add_place(place, true),
                    Some(Rvalue::Len { place, .. })
                        if matches!(body.place_ty(place, self.sigs).kind(), TyKind::Array(..)) =>
                    {
                        true
                    }
                    Some(Rvalue::Discriminant(place)) => add_place(place, false),
                    _ => false,
                }
            };
            if !eligible {
                continue;
            }
            pending[i] = deps.len();
            if deps.is_empty() {
                flags[i] |= PURE;
                ready.push(local);
            }
            for dep in deps {
                uses[dep.index()].push(local);
            }
        }
        while let Some(local) = ready.pop() {
            for &user in &uses[local.index()] {
                let i = user.index();
                pending[i] -= 1;
                if pending[i] == 0 {
                    flags[i] |= PURE;
                    ready.push(user);
                }
            }
        }
        let flags: Rc<[_]> = flags.into();
        self.promotion_flags.insert(key, flags.clone());
        flags
    }

    fn local_origin(&mut self, local: crate::Local, mutable: bool) -> LocalOrigin {
        let frame = self.local_borrows.last_mut().unwrap();
        frame.resize(frame.len().max(local.index() + 1), None);
        *frame[local.index()].get_or_insert_with(|| {
            let origin = LocalOrigin {
                token: self.next_borrow,
                mutable,
            };
            self.next_borrow += 1;
            self.live_borrows.insert(origin.token);
            origin
        })
    }

    fn projected_value(
        &mut self,
        locals: &[Option<Value>],
        value: &Value,
        projection: &[ProjectionElem],
        range: TextRange,
    ) -> Result<Value, EvalError> {
        let mut reader = |pointer: &Value, source, target, range| {
            self.pointer_value(pointer, source, target, range)
        };
        project_with_reader(
            locals,
            value,
            projection,
            range,
            &mut BorrowOrigin::default(),
            &mut reader,
        )
    }

    /// The initial value of static `id`, read at `range`. A constant may
    /// not read a `static mut`, whose value may change; a static's
    /// initializer reads its initial value.
    fn static_value(&mut self, id: ConstId, range: TextRange) -> Result<Value, EvalError> {
        let in_static = self
            .in_progress
            .first()
            .is_some_and(|c| self.items.consts[c.konst].is_static.is_some());
        if !in_static
            && (self.items.consts[id].is_static_mut()
                || flux_ty::traits::has_interior_mutability(
                    self.items,
                    self.sigs,
                    self.sigs.consts[id],
                ))
        {
            return Err(error(
                range,
                "constant accesses mutable global memory".to_owned(),
            ));
        }
        let outermost = self.in_progress.len() == 1;
        self.eval_const(id).map_err(|e| used(e, range, outermost))
    }

    /// Why writing to a static at compile time fails, as rustc says.
    fn static_write(&self) -> &'static str {
        let in_static = self
            .in_progress
            .first()
            .is_some_and(|c| self.items.consts[c.konst].is_static.is_some());
        if in_static {
            "modifying a static's initial value from another static's initializer"
        } else {
            "constant accesses mutable global memory"
        }
    }

    /// The discriminant of variant `variant` of enum `strukt`, as bits:
    /// a literal's, or its constant's value plus the variants since.
    pub fn discriminant(
        &mut self,
        strukt: flux_hir::StructId,
        variant: u32,
    ) -> Result<u128, EvalError> {
        let int = flux_ty::discriminant_ty(self.items, strukt);
        let variants = self.items.structs[strukt]
            .variants
            .as_ref()
            .expect("an enum");
        match variants[variant as usize].discriminant {
            flux_hir::Discriminant::Value(value) => Ok(truncate(value as u128, int)),
            flux_hir::Discriminant::Const { konst, offset } => match self.eval_const(konst)? {
                Value::Int { bits, .. } => Ok(truncate(bits.wrapping_add(u128::from(offset)), int)),
                _ => unreachable!("discriminants are integers"),
            },
        }
    }

    fn is_type_intrinsic(&self, func: FunctionId) -> bool {
        let data = &self.items.functions[func];
        data.intrinsic && matches!(data.name.as_str(), "size_of" | "align_of" | "needs_drop")
    }

    /// `size_of::<T>()`, `align_of::<T>()` or `needs_drop::<T>()` into
    /// `dest`, then `target`: out of line, so that the frames of
    /// recursive evaluation stay small.
    #[inline(never)]
    fn type_intrinsic(
        &mut self,
        (func, generic_args, generics): (FunctionId, GenericArgs, GenericArgs),
        locals: &mut [Option<Value>],
        dest: &Place,
        target: Option<crate::BasicBlock>,
        range: TextRange,
    ) -> Result<crate::BasicBlock, EvalError> {
        let name = self.items.functions[func].name;
        let ty = subst_list(generic_args, generics)
            .first()
            .copied()
            .unwrap_or(Ty::ERROR);
        let value = if name.as_str() == "needs_drop" {
            Value::Bool(flux_ty::traits::needs_drop(self.items, self.sigs, &[], ty))
        } else {
            let layout = self.layout_of(ty, range)?;
            if !layout.exact {
                return Err(error(
                    range,
                    format!("`{name}` of `{ty}` is not supported yet"),
                ));
            }
            let value = if name.as_str() == "size_of" {
                layout.size
            } else {
                layout.align
            };
            Value::Int {
                bits: u128::from(value),
                ty: IntTy::Usize,
            }
        };
        write(locals, dest, value, range)?;
        target.ok_or_else(|| error(range, "entering unreachable code".to_owned()))
    }

    /// The layout of `ty`, with the discriminants of the enums it holds
    /// evaluated.
    fn layout_of(&mut self, ty: Ty, range: TextRange) -> Result<crate::layout::Layout, EvalError> {
        let mut enums = Vec::new();
        let mut stack = vec![ty];
        let mut seen = FxHashSet::default();
        while let Some(ty) = stack.pop() {
            if !seen.insert(ty) {
                continue;
            }
            match ty.kind() {
                flux_ty::TyKind::Array(elem, _) => stack.push(elem),
                flux_ty::TyKind::Adt(adt, _) => {
                    if self.items.structs[adt.strukt].has_const_discriminants() {
                        enums.push(adt.strukt);
                    }
                    stack.extend(self.sigs.contents(ty).unwrap_or_default());
                }
                _ => stack.extend(self.sigs.contents(ty).unwrap_or_default()),
            }
        }
        let mut known = crate::Discriminants::new();
        for strukt in enums {
            self.layout(strukt, range)?;
            if let Ok(values) = self.discriminants(strukt) {
                known.insert(strukt, values);
            }
        }
        let layouts = crate::layout::Layouts::new(self.items, self.sigs, &known);
        Ok((*layouts.of(ty)).clone())
    }

    /// Every discriminant of enum `strukt`, as bits, evaluated: what
    /// making a value of it or matching on it needs.
    pub fn discriminants(&mut self, strukt: flux_hir::StructId) -> Result<Box<[u128]>, EvalError> {
        if let Some(result) = self.discriminants.get(&strukt) {
            return result.clone();
        }
        let count = self.items.structs[strukt].variant_count() as u32;
        let result: Result<Box<[u128]>, EvalError> =
            (0..count).map(|v| self.discriminant(strukt, v)).collect();
        self.discriminants.insert(strukt, result.clone());
        result
    }

    /// The discriminants of enum `strukt`, needed at `range`; another
    /// constant's errors are its own.
    fn layout(&mut self, strukt: flux_hir::StructId, range: TextRange) -> Result<(), EvalError> {
        if !self.items.structs[strukt].has_const_discriminants() {
            return Ok(());
        }
        self.discriminants(strukt)
            .map(drop)
            .map_err(|e| used(e, range, false))
    }

    /// The discriminants of the enums `body` matches on, which rustc
    /// needs to build it.
    fn switched(&mut self, body: &Body) -> Result<(), EvalError> {
        for data in body.blocks.values() {
            if let TerminatorKind::SwitchInt {
                variants: Some(strukt),
                ..
            } = data.terminator.kind
            {
                self.layout(strukt, data.terminator.range)?;
            }
        }
        Ok(())
    }

    /// Runs `body`, of an item with generic arguments `generics`;
    /// `checked` if overflow there is an error.
    fn run(
        &mut self,
        body: &'a Body,
        mut args: Vec<Value>,
        generics: GenericArgs,
        depth: u32,
        checked: bool,
    ) -> Result<Value, EvalError> {
        self.promoted.push(&body.promoted);
        let mut locals = self.frames.pop().unwrap_or_default();
        locals.resize(body.locals.len(), None);
        for (param, value) in body.params().zip(args.drain(..)) {
            locals[param.index()] = Some(value);
        }
        self.arguments.push(args);
        self.local_borrows
            .push(self.borrow_frames.pop().unwrap_or_default());
        let result = self.run_body(body, &mut locals, generics, depth, checked);
        self.promoted.pop();
        let mut borrows = self.local_borrows.pop().unwrap();
        for origin in borrows.drain(..).flatten() {
            self.live_borrows.remove(&origin.token);
        }
        self.borrow_frames.push(borrows);
        // No evaluated values survive in scratch storage: in particular, a
        // cached constant must not acquire an extra owner from a dead frame.
        locals.clear();
        self.frames.push(locals);
        result
    }

    fn run_body(
        &mut self,
        body: &Body,
        locals: &mut [Option<Value>],
        generics: GenericArgs,
        depth: u32,
        checked: bool,
    ) -> Result<Value, EvalError> {
        fn has_union(sigs: &Signatures, ty: Ty, seen: &mut FxHashSet<Ty>) -> bool {
            if !seen.insert(ty) {
                return false;
            }
            match ty.kind() {
                TyKind::Adt(adt, _) if sigs.unions.contains(&adt.strukt) => true,
                TyKind::Ref(_, t)
                | TyKind::RawPtr(_, t)
                | TyKind::Array(t, _)
                | TyKind::Slice(t) => has_union(sigs, t, seen),
                _ => sigs
                    .contents(ty)
                    .is_some_and(|fs| fs.iter().any(|&t| has_union(sigs, t, seen))),
            }
        }
        let mut seen = FxHashSet::default();
        if body
            .locals
            .values()
            .any(|l| has_union(self.sigs, l.ty.subst(generics), &mut seen))
        {
            return Err(error(
                body.blocks[Body::entry()].terminator.range,
                "union constant evaluation is not supported yet".to_owned(),
            ));
        }
        let mut block = Body::entry();
        loop {
            let data = &body.blocks[block];
            for statement in &data.statements {
                self.steps += 1;
                if self.steps > STEP_LIMIT {
                    return Err(error(
                        statement.range,
                        "constant evaluation is taking a long time".to_owned(),
                    ));
                }
                if let StatementKind::StorageDead(local) = statement.kind
                    && let Some(origin) = self
                        .local_borrows
                        .last_mut()
                        .and_then(|frame| frame.get_mut(local.index()))
                        .and_then(Option::take)
                {
                    self.live_borrows.remove(&origin.token);
                }
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    if let Some(Some(Value::Static(_))) = locals.get(dest.local.index())
                        && !dest.projection.is_empty()
                    {
                        return Err(error(statement.range, self.static_write().to_owned()));
                    }
                    let value =
                        self.rvalue(body, locals, generics, rvalue, statement.range, !checked)?;
                    write(locals, dest, value, statement.range)?;
                }
            }
            let range = data.terminator.range;
            block = match &data.terminator.kind {
                // What a constant drops runs no code: anything else is
                // E0493, reported before evaluation.
                TerminatorKind::Goto(target) | TerminatorKind::Drop { target, .. } => *target,
                TerminatorKind::SwitchInt {
                    discr,
                    targets,
                    otherwise,
                    variants,
                } => {
                    let value = match self.operand(locals, generics, discr, range)? {
                        Value::Int { bits, .. } => bits,
                        Value::Bool(b) => u128::from(b),
                        Value::Aggregate(_)
                        | Value::Variant { .. }
                        | Value::Static(_)
                        | Value::StaticPart { .. }
                        | Value::RawAddr(_)
                        | Value::Ref(_)
                        | Value::InteriorRef(_)
                        | Value::PointerView { .. }
                        | Value::LocalRef { .. } => {
                            unreachable!("switches on scalars")
                        }
                    };
                    let mut found = *otherwise;
                    for &(v, target) in targets {
                        let v = match *variants {
                            Some(strukt) => self
                                .discriminant(strukt, v as u32)
                                .map_err(|e| used(e, range, false))?,
                            None => v,
                        };
                        if v == value {
                            found = target;
                            break;
                        }
                    }
                    found
                }
                TerminatorKind::Call {
                    func,
                    generic_args,
                    args,
                    dest,
                    target,
                } => {
                    if let Some(message) =
                        self.panic_message(*func, args, body, locals, generics, range)?
                    {
                        return Err(error(range, format!("evaluation panicked: {message}")));
                    }
                    // `size_of`, `align_of` and `needs_drop` are known
                    // here too.
                    if self.is_type_intrinsic(*func) {
                        let call = (*func, *generic_args, generics);
                        block = self.type_intrinsic(call, locals, dest, *target, range)?;
                        continue;
                    }
                    if self.items.functions[*func].intrinsic
                        && self.items.functions[*func].name.as_str() == "unreachable_unchecked"
                    {
                        return Err(error(range, "entering unreachable code".into()));
                    }
                    // Only code generation expands them.
                    if self.items.functions[*func].intrinsic {
                        return Err(error(
                            range,
                            "this operation is not supported in constants yet".to_owned(),
                        ));
                    }
                    let mut values = self.arguments.pop().unwrap_or_default();
                    for arg in args {
                        match self.operand(locals, generics, arg, range) {
                            Ok(value) => values.push(value),
                            Err(e) => {
                                values.clear();
                                self.arguments.push(values);
                                return Err(e);
                            }
                        }
                    }
                    let callee = flux_ty::Instance {
                        function: *func,
                        args: Ty::list(&subst_list(generic_args, generics)),
                    };
                    let value = self.call(callee, values, depth, range, checked)?;
                    write(locals, dest, value, range)?;
                    match target {
                        Some(target) => *target,
                        None => return Err(error(range, "entering unreachable code".to_owned())),
                    }
                }
                // Reported by type checking.
                TerminatorKind::CallIndirect { .. } => {
                    return Err(error(
                        range,
                        "function pointer calls are not allowed in constants".to_owned(),
                    ));
                }
                TerminatorKind::Assert {
                    cond,
                    expected,
                    msg,
                    target,
                } => {
                    let Value::Bool(value) = self.operand(locals, generics, cond, range)? else {
                        unreachable!("assertions test booleans")
                    };
                    if value != *expected {
                        let message = self.assert_message(locals, generics, msg, range)?;
                        return Err(error(range, message));
                    }
                    *target
                }
                TerminatorKind::Return => {
                    return locals[Body::RETURN.index()]
                        .take()
                        .ok_or_else(|| error(range, "constant has no value".to_owned()));
                }
                TerminatorKind::Unreachable => {
                    return Err(error(range, "entering unreachable code".to_owned()));
                }
            };
        }
    }

    /// Calls `func`, a `const fn`; errors inside it are reported at the
    /// call.
    fn call(
        &mut self,
        instance: flux_ty::Instance,
        args: Vec<Value>,
        depth: u32,
        range: TextRange,
        checked: bool,
    ) -> Result<Value, EvalError> {
        let instance = flux_ty::traits::resolve_instance(self.items, self.sigs, instance);
        let func: FunctionId = instance.function;
        let Some(callee) = self.functions.get(func) else {
            return Err(error(
                range,
                "calling this function in constants is not supported yet".to_owned(),
            ));
        };
        if !self.switched.contains(&func) {
            self.switched(callee).map_err(|e| at_call(e, range))?;
            self.switched.insert(func);
        }
        if depth >= DEPTH_LIMIT {
            return Err(error(
                range,
                "reached the configured maximum number of stack frames".to_owned(),
            ));
        }
        // A function's own arithmetic follows `-C overflow-checks`, off in
        // flux, unless it inherits its caller's.
        let checked = checked && callee.inherits_overflow_checks;
        self.run(callee, args, instance.args, depth + 1, checked)
            .map_err(|e| at_call(e, range))
    }

    fn assert_message(
        &mut self,
        locals: &[Option<Value>],
        generics: GenericArgs,
        msg: &AssertMessage,
        range: TextRange,
    ) -> Result<String, EvalError> {
        let mut show = |op: &Operand| -> Result<String, EvalError> {
            Ok(show(&self.operand(locals, generics, op, range)?))
        };
        Ok(match msg {
            AssertMessage::DivisionByZero(l) => format!("attempt to divide `{}` by zero", show(l)?),
            AssertMessage::RemainderByZero(l) => format!(
                "attempt to calculate the remainder of `{}` with a divisor of zero",
                show(l)?
            ),
            AssertMessage::DivisionOverflow(l, r) => {
                format!(
                    "attempt to compute `{} / {}`, which would overflow",
                    show(l)?,
                    show(r)?
                )
            }
            AssertMessage::RemainderOverflow(l, r) => {
                format!(
                    "attempt to compute `{} % {}`, which would overflow",
                    show(l)?,
                    show(r)?
                )
            }
            AssertMessage::BoundsCheck { len, index } => {
                let mut number = |op: &Operand| -> Result<u128, EvalError> {
                    match self.operand(locals, generics, op, range)? {
                        Value::Int { bits, .. } => Ok(bits),
                        _ => unreachable!("lengths and indices are integers"),
                    }
                };
                format!(
                    "index out of bounds: the length is {} but the index is {}",
                    number(len)?,
                    number(index)?
                )
            }
        })
    }

    /// The message of a call of one of `core`'s panics, if `func` is
    /// one whose message is known: `panic`'s, a `c"..."` or a `"..."`,
    /// or `panic_display`'s `&str` (`panic!("{}", s)`, `expect`). Out
    /// of line, as evaluation recurses through its caller.
    #[inline(never)]
    fn panic_message(
        &mut self,
        func: FunctionId,
        args: &[Operand],
        body: &Body,
        locals: &[Option<Value>],
        generics: GenericArgs,
        range: TextRange,
    ) -> Result<Option<String>, EvalError> {
        if Some(func) == self.items.panic_fn {
            let message = match args.first() {
                Some(Operand::Const(Constant::CStr(index))) => {
                    String::from_utf8_lossy(&body.c_strings[*index as usize]).into_owned()
                }
                Some(Operand::Const(Constant::Promoted { index, ty }))
                    if *ty == Ty::new(flux_ty::TyKind::Ref(flux_ty::Mutability::Not, Ty::STR)) =>
                {
                    let bytes: Vec<u8> = match body.promoted.get(*index as usize) {
                        Some(Value::Aggregate(bytes)) => bytes
                            .iter()
                            .map(|b| match b {
                                Value::Int { bits, .. } => *bits as u8,
                                _ => b'?',
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    String::from_utf8_lossy(&bytes).into_owned()
                }
                _ => "explicit panic".to_owned(),
            };
            return Ok(Some(message));
        }
        let display = self
            .items
            .lang_fns
            .get(&flux_intern::Symbol::intern("panic_display"));
        if display == Some(&func)
            && let Some(arg) = args.first()
        {
            let value = self.operand(locals, generics, arg, range)?;
            return self.panic_string(value, range);
        }
        Ok(None)
    }

    /// Formatting a panic reads live transient references just as ordinary
    /// evaluation does. These wrappers cannot be stripped without checking
    /// their frame tokens first.
    fn panic_string(
        &mut self,
        mut value: Value,
        range: TextRange,
    ) -> Result<Option<String>, EvalError> {
        for _ in 0..=DEPTH_LIMIT {
            value = match value {
                Value::LocalRef { .. } => {
                    self.pointer_value(&value, Ty::ERROR, Ty::ERROR, range)?
                }
                Value::PointerView {
                    pointer,
                    source,
                    target,
                } => self.pointer_value(&pointer, source, target, range)?,
                Value::Ref(inner) => (*inner).clone(),
                Value::Aggregate(_) => return Ok(str_value(&Value::Ref(Arc::new(value)))),
                _ => return Ok(None),
            };
        }
        Err(unsupported_view(range))
    }

    fn operand(
        &mut self,
        locals: &[Option<Value>],
        generics: GenericArgs,
        operand: &Operand,
        range: TextRange,
    ) -> Result<Value, EvalError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                // Through a static's address: its initial value.
                if let Some(Some(Value::Static(id))) = locals.get(place.local.index())
                    && place.projection.first() == Some(&ProjectionElem::Deref)
                {
                    let value = self.static_value(*id, range)?;
                    return self.projected_value(locals, &value, &place.projection[1..], range);
                }
                if let Some(Some(Value::StaticPart { id, path })) = locals.get(place.local.index())
                    && place.projection.first() == Some(&ProjectionElem::Deref)
                {
                    let mut value = self.static_value(*id, range)?;
                    for p in path.iter() {
                        value = match (p, value) {
                            (
                                StaticProjection::Field(i),
                                Value::Aggregate(fields) | Value::Variant { fields, .. },
                            ) => fields[*i as usize].clone(),
                            (StaticProjection::Index(i), Value::Aggregate(fields)) => {
                                fields[*i as usize].clone()
                            }
                            (
                                StaticProjection::Subslice { from, len },
                                Value::Aggregate(fields),
                            ) => {
                                let end = from.checked_add(*len).ok_or_else(|| {
                                    error(range, "static subslice is out of bounds".into())
                                })?;
                                let fields =
                                    fields.get(*from as usize..end as usize).ok_or_else(|| {
                                        error(range, "static subslice is out of bounds".into())
                                    })?;
                                Value::Aggregate(fields.into())
                            }
                            (StaticProjection::Downcast(_), value @ Value::Variant { .. }) => value,
                            _ => {
                                return Err(error(
                                    range,
                                    "static projection in constants is not supported yet".into(),
                                ));
                            }
                        };
                    }
                    return self.projected_value(locals, &value, &place.projection[1..], range);
                }
                let value = locals[place.local.index()]
                    .as_ref()
                    .ok_or_else(|| error(range, "use of an uninitialized value".into()))?;
                self.projected_value(locals, value, &place.projection, range)
            }
            Operand::Const(constant) => match *constant {
                Constant::Static { id, .. } => Ok(Value::Static(id)),
                Constant::Promoted { index, .. } => {
                    let value = self
                        .promoted
                        .last()
                        .and_then(|p| p.get(index as usize))
                        .cloned()
                        .ok_or_else(|| error(range, "a promoted constant is missing".to_owned()))?;
                    self.promoted_layouts(&value, range)?;
                    Ok(Value::Ref(Arc::new(value)))
                }
                Constant::Fn { .. } => Err(error(
                    range,
                    "function pointers in constants are not supported yet".to_owned(),
                )),
                Constant::Int { bits, ty } => Ok(Value::Int { bits, ty }),
                Constant::Bool(b) => Ok(Value::Bool(b)),
                Constant::Char(c) => Ok(Value::Int {
                    bits: u128::from(c),
                    ty: IntTy::U32,
                }),
                Constant::Unit | Constant::FnItem(_) => Ok(Value::unit()),
                Constant::CStr(_) => Err(error(
                    range,
                    "C strings in constants are not supported yet".to_owned(),
                )),
                Constant::TyConst { value, ty } => {
                    match Constant::of_ty_const(value.subst(generics), ty) {
                        Constant::TyConst { .. } => Err(error(
                            range,
                            "constant depends on a generic parameter".to_owned(),
                        )),
                        known => self.operand(locals, generics, &Operand::Const(known), range),
                    }
                }
                Constant::Item { id, args, .. } => {
                    let c = ConstInstance {
                        konst: id,
                        args: Ty::list(&subst_list(args, generics)),
                    };
                    let outermost = self.in_progress.len() == 1;
                    self.eval_instance(c).map_err(|e| used(e, range, outermost))
                }
            },
        }
    }

    /// Promotion bypasses aggregate evaluation. Constructing an enum
    /// still needs all its discriminants, including when nested in an
    /// array, so preserve layout errors and discriminant cycle checks.
    fn promoted_layouts(&mut self, value: &Value, range: TextRange) -> Result<(), EvalError> {
        match value {
            Value::Variant { strukt, fields, .. } => {
                self.layout(*strukt, range)?;
                for field in fields.iter() {
                    self.promoted_layouts(field, range)?;
                }
            }
            Value::Aggregate(fields) => {
                for field in fields.iter() {
                    self.promoted_layouts(field, range)?;
                }
            }
            Value::Ref(value) | Value::InteriorRef(value) | Value::LocalRef { value, .. } => {
                self.promoted_layouts(value, range)?
            }
            Value::PointerView { pointer, .. } => self.promoted_layouts(pointer, range)?,
            Value::Int { .. }
            | Value::Bool(_)
            | Value::Static(_)
            | Value::StaticPart { .. }
            | Value::RawAddr(_) => {}
        }
        Ok(())
    }

    fn rvalue(
        &mut self,
        body: &Body,
        locals: &[Option<Value>],
        generics: GenericArgs,
        rvalue: &Rvalue,
        range: TextRange,
        wrapping: bool,
    ) -> Result<Value, EvalError> {
        match rvalue {
            Rvalue::Use(op) => self.operand(locals, generics, op, range),
            Rvalue::Aggregate(kind, ops) => {
                if let AggregateKind::Variant(strukt, _) = *kind {
                    self.layout(strukt, range)?;
                }
                let fields = if ops.is_empty() {
                    Value::empty_fields()
                } else {
                    ops.iter()
                        .map(|op| self.operand(locals, generics, op, range))
                        .collect::<Result<Arc<[_]>, _>>()?
                };
                if let AggregateKind::Struct(strukt) = *kind
                    && flux_ty::is_lang_struct(self.items, strukt, "non_null")
                    && matches!(fields.first(), Some(Value::RawAddr(0)))
                {
                    return Err(error(range, "null pointer in `NonNull` constant".into()));
                }
                Ok(match *kind {
                    AggregateKind::Variant(strukt, index) => Value::Variant {
                        strukt,
                        index,
                        fields,
                    },
                    AggregateKind::Union(..) => {
                        return Err(error(
                            range,
                            "union constant evaluation is not supported yet".to_owned(),
                        ));
                    }
                    AggregateKind::Tuple
                    | AggregateKind::Struct(_)
                    | AggregateKind::Array(_)
                    | AggregateKind::Closure => Value::Aggregate(fields),
                })
            }
            Rvalue::Repeat(op, count) => {
                let value = self.operand(locals, generics, op, range)?;
                let count = count.subst(generics).as_usize().unwrap_or(0);
                if count > MAX_ELEMENTS {
                    return Err(error(
                        range,
                        "arrays this large in constants are not supported yet".to_owned(),
                    ));
                }
                Ok(Value::Aggregate(vec![value; count as usize].into()))
            }
            // An array's length is its type's; a slice is behind a
            // reference.
            Rvalue::Len { place, .. } => {
                let ty = body.place_ty(place, self.sigs).subst(generics);
                // Slice metadata is known from the stored array path. Reading
                // its length must not read an interior-mutable allocation.
                let stored = if place.projection.as_ref() == [ProjectionElem::Deref] {
                    match locals.get(place.local.index()).and_then(Option::as_ref) {
                        Some(Value::Static(id)) => Some(self.sigs.consts[*id]),
                        Some(Value::StaticPart { id, path }) => {
                            static_path_type(self.sigs, self.sigs.consts[*id], path)
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(len) = ty.array_len().or_else(|| stored.and_then(Ty::array_len)) {
                    Ok(Value::Int {
                        bits: u128::from(len),
                        ty: IntTy::Usize,
                    })
                } else {
                    match self.operand(locals, generics, &Operand::Copy(place.clone()), range)? {
                        Value::Aggregate(elements) => Ok(Value::Int {
                            bits: elements.len() as u128,
                            ty: IntTy::Usize,
                        }),
                        _ => Err(error(
                            range,
                            "references in constants are not supported yet".into(),
                        )),
                    }
                }
            }
            Rvalue::Discriminant(place) => {
                match self.operand(locals, generics, &Operand::Copy(place.clone()), range)? {
                    Value::Variant { strukt, index, .. } => {
                        let bits = self
                            .discriminant(strukt, index)
                            .map_err(|e| used(e, range, false))?;
                        Ok(Value::Int {
                            bits,
                            ty: flux_ty::discriminant_ty(self.items, strukt),
                        })
                    }
                    _ => unreachable!("discriminants are of enums"),
                }
            }
            // A shared reference is to a value that cannot change while
            // it is borrowed: a copy of it.
            Rvalue::Ref {
                mutability: flux_ty::Mutability::Not,
                place,
                ..
            } => {
                if place.projection.first() == Some(&ProjectionElem::Deref) {
                    if let Some(Some(value @ Value::PointerView { .. })) =
                        locals.get(place.local.index())
                    {
                        if place.projection.len() != 1 {
                            return Err(unsupported_view(range));
                        }
                        if let Value::PointerView {
                            pointer,
                            source,
                            target,
                        } = value
                        {
                            self.pointer_value(pointer, *source, *target, range)?;
                        }
                        let pointee = body.place_ty(place, self.sigs).subst(generics);
                        return Ok(pointer_view(
                            value.clone(),
                            body.locals[place.local].ty.subst(generics),
                            Ty::new(TyKind::Ref(flux_ty::Mutability::Not, pointee)),
                        ));
                    }
                    let origin = match locals.get(place.local.index()).and_then(Option::as_ref) {
                        Some(Value::Static(id)) => Some((*id, Vec::new())),
                        Some(Value::StaticPart { id, path }) => Some((*id, path.to_vec())),
                        _ => None,
                    };
                    if let Some((id, mut path)) = origin {
                        let root = Place {
                            local: place.local,
                            projection: vec![ProjectionElem::Deref].into(),
                        };
                        let view = flux_ty::traits::normalize(
                            self.items,
                            self.sigs,
                            &[],
                            body.place_ty(&root, self.sigs).subst(generics),
                        );
                        let stored = static_path_type(self.sigs, self.sigs.consts[id], &path);
                        let compatible = stored == Some(view)
                            || matches!((stored.map(Ty::kind), view.kind()),
                                (Some(TyKind::Array(a, _)), TyKind::Slice(b)) if a == b);
                        if !compatible {
                            return Err(error(range, "reborrowing reinterpreted static pointers in constants is not supported yet".into()));
                        }
                        for p in place.projection.iter().skip(1) {
                            path.push(match *p {
                                ProjectionElem::Field(i) => StaticProjection::Field(i),
                                ProjectionElem::Downcast(i) => StaticProjection::Downcast(i),
                                ProjectionElem::Index(local) => {
                                    StaticProjection::Index(index(locals, local) as u64)
                                }
                                ProjectionElem::ConstantIndex {
                                    offset, from_end, ..
                                } => {
                                    let stored =
                                        static_path_type(self.sigs, self.sigs.consts[id], &path);
                                    let Some(TyKind::Array(_, len)) = stored.map(Ty::kind) else {
                                        return Err(error(
                                            range,
                                            "static slice projections are not supported yet".into(),
                                        ));
                                    };
                                    let len = len.as_usize().ok_or_else(|| {
                                        error(range, "unknown static array length".into())
                                    })?;
                                    StaticProjection::Index(if from_end {
                                        len - u64::from(offset)
                                    } else {
                                        u64::from(offset)
                                    })
                                }
                                ProjectionElem::Subslice { from, to, from_end } => {
                                    let len =
                                        static_path_type(self.sigs, self.sigs.consts[id], &path)
                                            .and_then(Ty::array_len)
                                            .ok_or_else(|| {
                                                error(range, "unknown static array length".into())
                                            })?;
                                    let end = if from_end {
                                        len.checked_sub(u64::from(to))
                                    } else {
                                        Some(u64::from(to))
                                    };
                                    let len = end
                                        .filter(|&end| end <= len)
                                        .and_then(|end| end.checked_sub(u64::from(from)))
                                        .ok_or_else(|| {
                                            error(range, "static subslice is out of bounds".into())
                                        })?;
                                    StaticProjection::Subslice {
                                        from: u64::from(from),
                                        len,
                                    }
                                }
                                _ => {
                                    return Err(error(
                                        range,
                                        "nested static address projections are not supported yet"
                                            .into(),
                                    ));
                                }
                            });
                        }
                        return Ok(if path.is_empty() {
                            Value::Static(id)
                        } else {
                            Value::StaticPart {
                                id,
                                path: path.into(),
                            }
                        });
                    }
                }
                let mut origin = BorrowOrigin::default();
                let value = {
                    let root = locals[place.local.index()]
                        .as_ref()
                        .ok_or_else(|| error(range, "use of an uninitialized value".into()))?;
                    let mut reader = |pointer: &Value, source, target, range| {
                        if matches!(pointer, Value::LocalRef { .. }) {
                            self.pointer_value(pointer, source, target, range)
                        } else {
                            Err(unsupported_view(range))
                        }
                    };
                    project_with_reader(
                        locals,
                        root,
                        &place.projection,
                        range,
                        &mut origin,
                        &mut reader,
                    )?
                };
                if !place.projection.contains(&ProjectionElem::Deref) {
                    let mutable = match body.locals[place.local].kind {
                        crate::LocalKind::User { mutable, .. } => Some(mutable),
                        crate::LocalKind::Param => Some(false),
                        kind if kind.is_temp() => {
                            let flags = self.temporary_flags(body)[place.local.index()];
                            let ty = body.place_ty(place, self.sigs).subst(generics);
                            let promoted = flags & 2 != 0
                                || (flags & 1 != 0
                                    && !self.inline_interior(&value, ty)
                                    && !flux_ty::traits::needs_drop(
                                        self.items,
                                        self.sigs,
                                        &[],
                                        ty,
                                    ));
                            (!promoted).then_some(flags & 4 != 0)
                        }
                        _ => None,
                    };
                    if let Some(mutable) = mutable {
                        origin.local = Some(self.local_origin(place.local, mutable));
                    }
                }
                let ty = body.place_ty(place, self.sigs).subst(generics);
                // The structural evaluator cannot reinterpret this storage as
                // bytes. Keep the new primitive out of borrowed constant memory
                // until those reads can validate the source and target layouts.
                if self.inline_lang(&value, ty, &["unsafe_cell"]) {
                    return Err(error(
                        range,
                        "borrowing UnsafeCell in constants is not supported yet".into(),
                    ));
                }
                Ok(if let Some(origin) = origin.local {
                    Value::LocalRef {
                        value: Arc::new(value),
                        origin,
                        raw: false,
                    }
                } else if origin.interior || self.inline_interior(&value, ty) {
                    Value::InteriorRef(Arc::new(value))
                } else {
                    Value::Ref(Arc::new(value))
                })
            }
            // Reborrowing a promoted empty array or slice cannot mutate a
            // stored value. Keep the mutable-reference restriction for every
            // other pointee and for borrows of local array storage.
            Rvalue::Ref { place, .. }
                if place.projection.len() == 1
                    && place.projection.first() == Some(&ProjectionElem::Deref)
                    && match body.place_ty(place, self.sigs).subst(generics).kind() {
                        TyKind::Array(_, len) => len.as_usize() == Some(0),
                        TyKind::Slice(_) => true,
                        _ => false,
                    } =>
            {
                let value = self.operand(locals, generics, &Operand::Copy(place.clone()), range)?;
                match value {
                    Value::Aggregate(ref fields) if fields.is_empty() => {
                        Ok(Value::Ref(Arc::new(value)))
                    }
                    _ => Err(error(
                        range,
                        "mutable references in constants are not supported yet".to_owned(),
                    )),
                }
            }
            // A reference to an array as one to a slice holds the same
            // elements.
            Rvalue::Unsize(op, _) => {
                let value = self.operand(locals, generics, op, range)?;
                if matches!(value, Value::RawAddr(_) | Value::PointerView { .. }) {
                    return Err(error(
                        range,
                        "unsizing raw addresses or reinterpreted pointers in constants is not supported yet".into(),
                    ));
                }
                Ok(value)
            }
            // Made only once code is monomorphized.
            Rvalue::Subslice { .. } => Err(error(
                range,
                "slicing in constants is not supported yet".to_owned(),
            )),
            Rvalue::Ref { .. } => Err(error(
                range,
                "mutable references in constants are not supported yet".to_owned(),
            )),
            Rvalue::Cast(op, to) => {
                let value = self.operand(locals, generics, op, range)?;
                // A pointer as another points to the same value.
                if to.is_pointer() {
                    if matches!(to.kind(), TyKind::RawPtr(_, inner) if inner.is_unsized())
                        && matches!(value, Value::Int { .. } | Value::RawAddr(_))
                    {
                        return Err(error(
                            range,
                            "unsizing raw addresses or reinterpreted pointers in constants is not supported yet".into(),
                        ));
                    }
                    return match value {
                        Value::Int { bits, ty } if matches!(to.kind(), TyKind::RawPtr(..)) => {
                            Ok(Value::RawAddr(
                                truncate(sign_extend(bits, ty) as u128, IntTy::Usize) as u64,
                            ))
                        }
                        Value::RawAddr(_)
                        | Value::Ref(_)
                        | Value::InteriorRef(_)
                        | Value::Static(_)
                        | Value::StaticPart { .. }
                        | Value::PointerView { .. }
                        | Value::LocalRef { .. } => Ok(pointer_view(
                            value,
                            flux_ty::traits::normalize(
                                self.items,
                                self.sigs,
                                &[],
                                body.operand_ty(op, self.sigs).subst(generics),
                            ),
                            flux_ty::traits::normalize(
                                self.items,
                                self.sigs,
                                &[],
                                to.subst(generics),
                            ),
                        )),
                        _ => Err(error(
                            range,
                            "integers cast to pointers in constants are not supported yet"
                                .to_owned(),
                        )),
                    };
                }
                if let Value::Ref(_)
                | Value::InteriorRef(_)
                | Value::Static(_)
                | Value::StaticPart { .. }
                | Value::RawAddr(_)
                | Value::PointerView { .. }
                | Value::LocalRef { .. } = value
                {
                    return Err(error(
                        range,
                        "pointers cannot be cast to integers during const eval".to_owned(),
                    ));
                }
                let bits = match value {
                    Value::Int { bits, ty } => sign_extend(bits, ty) as u128,
                    Value::Bool(b) => u128::from(b),
                    Value::Aggregate(_)
                    | Value::Variant { .. }
                    | Value::Static(_)
                    | Value::StaticPart { .. }
                    | Value::RawAddr(_)
                    | Value::Ref(_)
                    | Value::InteriorRef(_)
                    | Value::PointerView { .. }
                    | Value::LocalRef { .. } => {
                        unreachable!("casts are of scalars")
                    }
                };
                let to = to.scalar_int().expect("casts are to integers or `char`");
                Ok(Value::Int {
                    bits: truncate(bits, to),
                    ty: to,
                })
            }
            Rvalue::UnaryOp(op, operand) => {
                match (op, self.operand(locals, generics, operand, range)?) {
                    (UnOp::Not, Value::Bool(b)) => Ok(Value::Bool(!b)),
                    (UnOp::Not, Value::Int { bits, ty }) => Ok(Value::Int {
                        bits: truncate(!bits, ty),
                        ty,
                    }),
                    (UnOp::Neg, value @ Value::Int { bits, ty }) => {
                        if ty.is_signed() && bits == min_bits(ty) && !wrapping {
                            return Err(error(
                                range,
                                format!(
                                    "attempt to negate `{}`, which would overflow",
                                    show(&value)
                                ),
                            ));
                        }
                        Ok(Value::Int {
                            bits: truncate(bits.wrapping_neg(), ty),
                            ty,
                        })
                    }
                    _ => unreachable!("type checking allows no other unary operations"),
                }
            }
            Rvalue::BinaryOp(op, l, r) => {
                // Pointer values do not carry concrete runtime addresses here.
                // Never compare their referents or synthesize an address order.
                if matches!(body.operand_ty(l, self.sigs).kind(), TyKind::RawPtr(..)) {
                    return Err(error(
                        range,
                        "pointers cannot be reliably compared during const eval".to_owned(),
                    ));
                }
                let l = self.operand(locals, generics, l, range)?;
                let r = self.operand(locals, generics, r, range)?;
                let result = if wrapping {
                    wrapping_binary(*op, &l, &r)
                } else {
                    binary(*op, &l, &r)
                };
                result.map_err(|message| error(range, message))
            }
        }
    }
}

fn error(range: TextRange, message: String) -> EvalError {
    EvalError {
        range,
        message,
        nested: false,
        cycle: None,
    }
}

fn unsupported_view(range: TextRange) -> EvalError {
    error(
        range,
        "this pointer reinterpretation in constants is not supported yet".into(),
    )
}

fn pointer_pointee(ty: Ty) -> Option<Ty> {
    match ty.kind() {
        TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) => Some(pointee),
        _ => None,
    }
}

fn pointer_view(value: Value, source: Ty, target: Ty) -> Value {
    if matches!(value, Value::RawAddr(_)) {
        return value;
    }
    let value = match value {
        Value::LocalRef { value, origin, raw } => Value::LocalRef {
            value,
            origin,
            raw: raw || matches!(target.kind(), TyKind::RawPtr(..)),
        },
        value => value,
    };
    let (pointer, source) = match value {
        Value::PointerView {
            pointer, source, ..
        } => (pointer, source),
        value => (Arc::new(value), source),
    };
    let stored = pointer_pointee(source);
    let viewed = pointer_pointee(target);
    // A str and its byte slice have the same initialized storage and length.
    // The reverse direction still needs UTF-8 validation when referenced.
    let bytes_of_str = matches!((stored.map(Ty::kind), viewed.map(Ty::kind)),
        (Some(TyKind::Str), Some(TyKind::Slice(elem))) if elem == Ty::int(IntTy::U8));
    if stored == viewed || bytes_of_str {
        (*pointer).clone()
    } else {
        Value::PointerView {
            pointer,
            source,
            target,
        }
    }
}

/// `e`, from a constant used at `range`: reported there if it is a
/// cycle, which is reported at every constant in it, or the depth
/// limit, reported once, at the `outermost` constant; otherwise the
/// constant's own. A cycle through a discriminant is reported at it.
fn used(e: EvalError, range: TextRange, outermost: bool) -> EvalError {
    if e.cycle.is_some() {
        return e;
    }
    let nested = if e.message.starts_with("queries overflow") {
        !outermost
    } else {
        !e.message.starts_with("cycle detected")
    };
    EvalError { range, nested, ..e }
}

/// `e`, from inside a call: reported at the call, unless it is a cycle
/// through a discriminant.
fn at_call(e: EvalError, range: TextRange) -> EvalError {
    if e.cycle.is_some() {
        e
    } else {
        EvalError { range, ..e }
    }
}

/// The value of the index in `local`, checked against the length before.
fn index(locals: &[Option<Value>], local: crate::Local) -> usize {
    match locals[local.index()] {
        Some(Value::Int { bits, .. }) => bits as usize,
        _ => unreachable!("indices are integers"),
    }
}

/// Physical source-field index for repeated typed reads. Sorting once avoids
/// rescanning every field for each element of a reinterpreted aggregate.
struct MemoryField {
    at: u64,
    end: u64,
    index: u32,
    ty: Ty,
}

/// Identity of readonly frame storage. Kept separate from copied value data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalOrigin {
    token: u64,
    mutable: bool,
}

#[derive(Default)]
struct BorrowOrigin {
    interior: bool,
    local: Option<LocalOrigin>,
}

#[cfg(test)]
fn read(locals: &[Option<Value>], place: &Place, range: TextRange) -> Result<Value, EvalError> {
    read_with_origin(locals, place, range, &mut BorrowOrigin::default())
}

#[cfg(test)]
fn read_with_origin(
    locals: &[Option<Value>],
    place: &Place,
    range: TextRange,
    origin: &mut BorrowOrigin,
) -> Result<Value, EvalError> {
    let value = locals[place.local.index()]
        .as_ref()
        .ok_or_else(|| error(range, "use of an uninitialized value".to_owned()))?;
    project_with_origin(locals, value, &place.projection, range, origin)
}

/// The part of `value` at `projection`, whose indices are in `locals`.
#[cfg(test)]
fn project(
    locals: &[Option<Value>],
    value: &Value,
    projection: &[ProjectionElem],
    range: TextRange,
) -> Result<Value, EvalError> {
    project_with_origin(
        locals,
        value,
        projection,
        range,
        &mut BorrowOrigin::default(),
    )
}

/// Preserve origin whenever a projection dereferences interior-mutable
/// temporary storage, including references nested in aggregates or references.
#[cfg(test)]
fn project_with_origin(
    locals: &[Option<Value>],
    value: &Value,
    projection: &[ProjectionElem],
    range: TextRange,
    origin: &mut BorrowOrigin,
) -> Result<Value, EvalError> {
    let mut reader = |_: &Value, _: Ty, _: Ty, range| Err(unsupported_view(range));
    project_with_reader(locals, value, projection, range, origin, &mut reader)
}

type ViewReader<'a> = dyn FnMut(&Value, Ty, Ty, TextRange) -> Result<Value, EvalError> + 'a;

fn project_with_reader(
    locals: &[Option<Value>],
    mut value: &Value,
    projection: &[ProjectionElem],
    range: TextRange,
    origin: &mut BorrowOrigin,
    reader: &mut ViewReader<'_>,
) -> Result<Value, EvalError> {
    for (k, elem) in projection.iter().enumerate() {
        value = match (elem, value) {
            (
                ProjectionElem::Field(i),
                Value::Aggregate(fields) | Value::Variant { fields, .. },
            ) => &fields[*i as usize],
            (ProjectionElem::Index(local), Value::Aggregate(elements)) => {
                &elements[index(locals, *local)]
            }
            (
                &ProjectionElem::ConstantIndex {
                    offset, from_end, ..
                },
                Value::Aggregate(elements),
            ) => &elements[constant_index(elements.len(), offset, from_end)],
            // A new array of the elements, projected further.
            (&ProjectionElem::Subslice { from, to, from_end }, Value::Aggregate(elements)) => {
                let end = if from_end {
                    elements.len() - to as usize
                } else {
                    to as usize
                };
                let part = Value::Aggregate(elements[from as usize..end].into());
                return project_with_reader(
                    locals,
                    &part,
                    &projection[k + 1..],
                    range,
                    origin,
                    reader,
                );
            }
            (ProjectionElem::Downcast(_), Value::Variant { .. }) => value,
            (ProjectionElem::Deref, Value::InteriorRef(pointee)) => {
                *origin = BorrowOrigin {
                    interior: true,
                    local: None,
                };
                pointee
            }
            (ProjectionElem::Deref, Value::Ref(pointee)) => {
                *origin = BorrowOrigin::default();
                pointee
            }
            (ProjectionElem::Deref, reference @ Value::LocalRef { origin: local, .. }) => {
                *origin = BorrowOrigin {
                    interior: false,
                    local: Some(*local),
                };
                let viewed = reader(reference, Ty::ERROR, Ty::ERROR, range)?;
                return project_with_reader(
                    locals,
                    &viewed,
                    &projection[k + 1..],
                    range,
                    origin,
                    reader,
                );
            }
            (
                ProjectionElem::Deref,
                Value::PointerView {
                    pointer,
                    source,
                    target,
                },
            ) => {
                *origin = BorrowOrigin {
                    interior: matches!(**pointer, Value::InteriorRef(_)),
                    local: match **pointer {
                        Value::LocalRef { origin, .. } => Some(origin),
                        _ => None,
                    },
                };
                let viewed = reader(pointer, *source, *target, range)?;
                return project_with_reader(
                    locals,
                    &viewed,
                    &projection[k + 1..],
                    range,
                    origin,
                    reader,
                );
            }
            _ => {
                return Err(error(
                    range,
                    "references in constants are not supported yet".to_owned(),
                ));
            }
        };
    }
    Ok(value.clone())
}

fn write(
    locals: &mut [Option<Value>],
    place: &Place,
    new: Value,
    range: TextRange,
) -> Result<(), EvalError> {
    if place.projection.is_empty() {
        locals[place.local.index()] = Some(new);
        return Ok(());
    }
    // Most places have just a few projections; dynamic indices must be
    // read before mutably borrowing the destination's local.
    let mut inline = [0; 16];
    let mut spill = Vec::new();
    let indices = if place.projection.len() <= inline.len() {
        &mut inline[..place.projection.len()]
    } else {
        spill.resize(place.projection.len(), 0);
        &mut spill[..]
    };
    for (elem, i) in place.projection.iter().zip(indices.iter_mut()) {
        if let ProjectionElem::Index(local) = elem {
            *i = index(locals, *local);
        }
    }
    if place
        .projection
        .iter()
        .any(|e| matches!(e, ProjectionElem::Subslice { .. }))
    {
        return Err(error(
            range,
            "writing a slice pattern's rest in constants is not supported yet".to_owned(),
        ));
    }
    let slot = &mut locals[place.local.index()];
    let mut value = slot
        .as_mut()
        .ok_or_else(|| error(range, "use of an uninitialized value".to_owned()))?;
    for (elem, &i) in place.projection.iter().zip(indices.iter()) {
        value = match (elem, value) {
            (
                ProjectionElem::Field(i),
                Value::Aggregate(fields) | Value::Variant { fields, .. },
            ) => &mut Arc::make_mut(fields)[*i as usize],
            (ProjectionElem::Index(_), Value::Aggregate(elements)) => {
                &mut Arc::make_mut(elements)[i]
            }
            (
                &ProjectionElem::ConstantIndex {
                    offset, from_end, ..
                },
                Value::Aggregate(elements),
            ) => {
                let at = constant_index(elements.len(), offset, from_end);
                &mut Arc::make_mut(elements)[at]
            }
            (ProjectionElem::Downcast(_), value @ Value::Variant { .. }) => value,
            _ => {
                return Err(error(
                    range,
                    "references in constants are not supported yet".to_owned(),
                ));
            }
        };
    }
    *value = new;
    Ok(())
}

/// `l op r`; an error message if it would overflow or divide by zero.
pub fn binary(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let (lb, rb, ty) = match (l, r) {
        (Value::Bool(a), Value::Bool(b)) => {
            let value = match op {
                BinOp::BitAnd => *a & *b,
                BinOp::BitOr => *a | *b,
                BinOp::BitXor => *a ^ *b,
                BinOp::Eq => a == b,
                BinOp::Ne => a != b,
                BinOp::Lt => !*a & *b,
                BinOp::Le => a <= b,
                BinOp::Gt => *a & !*b,
                BinOp::Ge => a >= b,
                _ => unreachable!("type checking allows no other `bool` operations"),
            };
            return Ok(Value::Bool(value));
        }
        (Value::Aggregate(a), Value::Aggregate(b)) if a.is_empty() && b.is_empty() => {
            let equal = matches!(op, BinOp::Eq | BinOp::Le | BinOp::Ge);
            return Ok(Value::Bool(equal));
        }
        (Value::Int { bits: a, ty }, Value::Int { bits: b, .. }) => (*a, *b, *ty),
        _ => unreachable!("operands of the same type"),
    };
    let signed = ty.is_signed();
    let (a, b) = (sign_extend(lb, ty), sign_extend(rb, ty));
    let int = |value: i128| Value::Int {
        bits: truncate(value as u128, ty),
        ty,
    };
    let overflow = |sym: &str| {
        format!(
            "attempt to compute `{} {sym} {}`, which would overflow",
            show(l),
            show(r)
        )
    };
    // The exact result, to compare against the type's range.
    let fits = |exact: Option<i128>, exact_unsigned: Option<u128>| {
        if signed {
            exact.filter(|&v| v >= min_value(ty) && v <= max_value(ty))
        } else {
            exact_unsigned.filter(|&v| v <= ty.max()).map(|v| v as i128)
        }
    };
    let (ua, ub) = (lb, truncate(rb, ty));
    let value = match op {
        BinOp::Add => fits(a.checked_add(b), ua.checked_add(ub)).ok_or_else(|| overflow("+"))?,
        BinOp::Sub => fits(a.checked_sub(b), ua.checked_sub(ub)).ok_or_else(|| overflow("-"))?,
        BinOp::Mul => fits(a.checked_mul(b), ua.checked_mul(ub)).ok_or_else(|| overflow("*"))?,
        BinOp::Div | BinOp::Rem => {
            if b == 0 {
                return Err(match op {
                    BinOp::Div => format!("attempt to divide `{}` by zero", show(l)),
                    _ => format!(
                        "attempt to calculate the remainder of `{}` with a divisor of zero",
                        show(l)
                    ),
                });
            }
            if signed && a == min_value(ty) && b == -1 {
                return Err(overflow(if op == BinOp::Div { "/" } else { "%" }));
            }
            match (op, signed) {
                (BinOp::Div, true) => a / b,
                (BinOp::Div, false) => (ua / ub) as i128,
                (_, true) => a % b,
                (_, false) => (ua % ub) as i128,
            }
        }
        BinOp::Shl | BinOp::Shr => {
            // The amount may have any integer type.
            let amount = match r {
                Value::Int { bits, ty } => sign_extend(*bits, *ty),
                _ => unreachable!("shift amounts are integers"),
            };
            if amount < 0 || amount >= i128::from(ty.bits()) {
                let way = if op == BinOp::Shl { "left" } else { "right" };
                return Err(format!(
                    "attempt to shift {way} by `{}`, which would overflow",
                    show(r)
                ));
            }
            match (op, signed) {
                (BinOp::Shl, _) => (ua << amount) as i128,
                (_, true) => a >> amount,
                (_, false) => (ua >> amount) as i128,
            }
        }
        BinOp::BitAnd => (ua & ub) as i128,
        BinOp::BitOr => (ua | ub) as i128,
        BinOp::BitXor => (ua ^ ub) as i128,
        BinOp::Eq => return Ok(Value::Bool(a == b)),
        BinOp::Ne => return Ok(Value::Bool(a != b)),
        BinOp::Lt => return Ok(Value::Bool(if signed { a < b } else { ua < ub })),
        BinOp::Le => return Ok(Value::Bool(if signed { a <= b } else { ua <= ub })),
        BinOp::Gt => return Ok(Value::Bool(if signed { a > b } else { ua > ub })),
        BinOp::Ge => return Ok(Value::Bool(if signed { a >= b } else { ua >= ub })),
    };
    Ok(int(value))
}

/// `l op r` with flux's run-time semantics: arithmetic wraps and shift
/// amounts are masked; only division by zero is an error.
pub fn wrapping_binary(op: BinOp, l: &Value, r: &Value) -> Result<Value, String> {
    let (Value::Int { bits: a, ty }, Value::Int { bits: b, ty: rty }) = (l, r) else {
        return binary(op, l, r);
    };
    let (a, b, ty) = (*a, *b, *ty);
    let int = |bits: u128| Value::Int {
        bits: truncate(bits, ty),
        ty,
    };
    Ok(match op {
        BinOp::Add => int(a.wrapping_add(b)),
        BinOp::Sub => int(a.wrapping_sub(b)),
        BinOp::Mul => int(a.wrapping_mul(b)),
        BinOp::Shl | BinOp::Shr => {
            let amount = (truncate(b, *rty) % u128::from(ty.bits())) as u32;
            let amount_value = Value::Int {
                bits: u128::from(amount),
                ty: IntTy::U32,
            };
            return binary(op, l, &amount_value);
        }
        BinOp::Div | BinOp::Rem
            if ty.is_signed() && a == min_bits(ty) && sign_extend(b, ty) == -1 =>
        {
            // `MIN / -1` wraps to `MIN`, and `MIN % -1` is 0.
            if op == BinOp::Div { int(a) } else { int(0) }
        }
        _ => return binary(op, l, r),
    })
}

/// A value as rustc writes it in messages: `5_i32`, `-1_i8`, `i32::MAX`,
/// `u8::MAX`.
pub fn show(value: &Value) -> String {
    match *value {
        Value::Int { bits, ty } => {
            let name = ty.name();
            if bits == truncate(ty.max(), ty) {
                format!("{name}::MAX")
            } else if ty.is_signed() && bits == min_bits(ty) {
                format!("{name}::MIN")
            } else if ty.is_signed() {
                format!("{}_{name}", sign_extend(bits, ty))
            } else {
                format!("{bits}_{name}")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::Aggregate(_) | Value::Variant { .. } => "{aggregate}".to_owned(),
        Value::Static(_) | Value::StaticPart { .. } => "{static}".to_owned(),
        Value::RawAddr(address) => format!("0x{address:x}"),
        Value::PointerView { ref pointer, .. } => show(pointer),
        Value::LocalRef { ref value, .. } => format!("&{}", show(value)),
        Value::Ref(ref pointee) | Value::InteriorRef(ref pointee) => format!("&{}", show(pointee)),
    }
}

fn mask(ty: IntTy) -> u128 {
    if ty.bits() == 128 {
        u128::MAX
    } else {
        (1u128 << ty.bits()) - 1
    }
}

pub fn truncate(bits: u128, ty: IntTy) -> u128 {
    bits & mask(ty)
}

pub fn sign_extend(bits: u128, ty: IntTy) -> i128 {
    let width = ty.bits();
    if !ty.is_signed() || width == 128 {
        return bits as i128;
    }
    let shift = 128 - width;
    ((bits << shift) as i128) >> shift
}

fn min_bits(ty: IntTy) -> u128 {
    1u128 << (ty.bits() - 1)
}

fn min_value(ty: IntTy) -> i128 {
    if ty.bits() == 128 {
        i128::MIN
    } else {
        -(1i128 << (ty.bits() - 1))
    }
}

fn max_value(ty: IntTy) -> i128 {
    if ty.bits() == 128 {
        i128::MAX
    } else {
        (1i128 << (ty.bits() - 1)) - 1
    }
}

/// The text of `value`, a `str` behind references, if it is one.
fn str_value(value: &Value) -> Option<String> {
    match value {
        Value::Ref(inner) => match &**inner {
            Value::Aggregate(bytes) => bytes
                .iter()
                .map(|b| match b {
                    Value::Int {
                        bits,
                        ty: IntTy::U8,
                    } => Some(*bits as u8),
                    _ => None,
                })
                .collect::<Option<Vec<u8>>>()
                .and_then(|bytes| String::from_utf8(bytes).ok()),
            inner @ Value::Ref(_) => str_value(inner),
            _ => None,
        },
        _ => None,
    }
}

/// The index a [`ProjectionElem::ConstantIndex`] names in `len` elements.
fn constant_index(len: usize, offset: u32, from_end: bool) -> usize {
    if from_end {
        len - offset as usize
    } else {
        offset as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(bits: u128) -> Value {
        Value::Int {
            bits,
            ty: IntTy::I32,
        }
    }

    #[test]
    fn aggregate_clones_share_storage_until_nested_projection_writes() {
        let row = Value::Aggregate(vec![int(1), int(2)].into());
        let cached = Value::Aggregate(vec![row.clone(), row].into());
        let Value::Aggregate(original) = &cached else {
            unreachable!()
        };
        let mut locals = vec![Some(cached.clone()), Some(int(1))];
        let Some(Value::Aggregate(copy)) = &locals[0] else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(original, copy));
        let root = Place::from(crate::Local::from_raw(0));
        let place = root.index(crate::Local::from_raw(1)).field(0);
        write(&mut locals, &place, int(9), TextRange::default()).unwrap();
        assert_eq!(read(&locals, &place, TextRange::default()).unwrap(), int(9));
        assert_eq!(
            project(&locals, &cached, &place.projection, TextRange::default()).unwrap(),
            int(1)
        );
        // Mutating one repeated row must not change its sibling.
        assert_eq!(
            read(&locals, &root.field(0).field(0), TextRange::default()).unwrap(),
            int(1)
        );
        let Some(Value::Aggregate(copy)) = &locals[0] else {
            unreachable!()
        };
        assert!(!Arc::ptr_eq(original, copy));
        let (Value::Aggregate(left), Value::Aggregate(right)) = (&original[0], &copy[0]) else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(left, right));
    }

    #[test]
    fn reference_clones_share_pointee_and_empty_values_share_storage() {
        let value = Value::Ref(Arc::new(Value::Aggregate(vec![int(7)].into())));
        let Value::Ref(original) = &value else {
            unreachable!()
        };
        let Value::Ref(copy) = value.clone() else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(original, &copy));
        let Value::Aggregate(first) = Value::unit() else {
            unreachable!()
        };
        let Value::Aggregate(second) = Value::unit() else {
            unreachable!()
        };
        assert!(Arc::ptr_eq(&first, &second));
    }
}
