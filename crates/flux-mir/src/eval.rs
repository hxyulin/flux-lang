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
    /// A shared reference to a value: what it points to, which cannot
    /// change while it is borrowed. A reference to a slice holds the
    /// elements as an aggregate.
    Ref(Arc<Value>),
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
        self.steps = 0;
        // A constant's own arithmetic is always checked.
        let result = self
            .switched(body)
            .and_then(|()| self.run(body, Vec::new(), c.args, 0, true));
        self.steps = steps;
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

    /// The initial value of static `id`, read at `range`. A constant may
    /// not read a `static mut`, whose value may change; a static's
    /// initializer reads its initial value.
    fn static_value(&mut self, id: ConstId, range: TextRange) -> Result<Value, EvalError> {
        let in_static = self
            .in_progress
            .first()
            .is_some_and(|c| self.items.consts[c.konst].is_static.is_some());
        if self.items.consts[id].is_static_mut() && !in_static {
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
        let result = self.run_body(body, &mut locals, generics, depth, checked);
        self.promoted.pop();
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
                        | Value::Ref(_) => {
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
            return Ok(str_value(&value));
        }
        Ok(None)
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
                    return project(locals, &value, &place.projection[1..], range);
                }
                read(locals, place, range)
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
            Value::Ref(value) => self.promoted_layouts(value, range)?,
            Value::Int { .. } | Value::Bool(_) | Value::Static(_) => {}
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
            Rvalue::Len { place, .. } => match body.place_ty(place, self.sigs).kind() {
                flux_ty::TyKind::Array(_, len) => Ok(Value::Int {
                    bits: u128::from(len.subst(generics).as_usize().unwrap_or(0)),
                    ty: IntTy::Usize,
                }),
                // A slice's, behind a reference: its elements.
                _ => match self.operand(locals, generics, &Operand::Copy(place.clone()), range)? {
                    Value::Aggregate(elements) => Ok(Value::Int {
                        bits: elements.len() as u128,
                        ty: IntTy::Usize,
                    }),
                    _ => Err(error(
                        range,
                        "references in constants are not supported yet".to_owned(),
                    )),
                },
            },
            Rvalue::Discriminant(place) => match read(locals, place, range)? {
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
            },
            // A shared reference is to a value that cannot change while
            // it is borrowed: a copy of it.
            Rvalue::Ref {
                mutability: flux_ty::Mutability::Not,
                place,
                ..
            } => {
                let value = self.operand(locals, generics, &Operand::Copy(place.clone()), range)?;
                Ok(Value::Ref(Arc::new(value)))
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
            Rvalue::Unsize(op, _) => self.operand(locals, generics, op, range),
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
                    return match value {
                        Value::Ref(_) | Value::Static(_) => Ok(value),
                        _ => Err(error(
                            range,
                            "integers cast to pointers in constants are not supported yet"
                                .to_owned(),
                        )),
                    };
                }
                if let Value::Ref(_) | Value::Static(_) = value {
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
                    | Value::Ref(_) => {
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

fn read(locals: &[Option<Value>], place: &Place, range: TextRange) -> Result<Value, EvalError> {
    let value = locals[place.local.index()]
        .as_ref()
        .ok_or_else(|| error(range, "use of an uninitialized value".to_owned()))?;
    project(locals, value, &place.projection, range)
}

/// The part of `value` at `projection`, whose indices are in `locals`.
fn project(
    locals: &[Option<Value>],
    mut value: &Value,
    projection: &[ProjectionElem],
    range: TextRange,
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
                return project(locals, &part, &projection[k + 1..], range);
            }
            (ProjectionElem::Downcast(_), Value::Variant { .. }) => value,
            (ProjectionElem::Deref, Value::Ref(pointee)) => pointee,
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
        Value::Static(_) => "{static}".to_owned(),
        Value::Ref(ref pointee) => format!("&{}", show(pointee)),
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
