//! MIR: each function as a control-flow graph of basic blocks over locals.
//!
//! MIR is where Rust's runtime semantics become explicit, so later stages
//! (code generation, and borrow checking once there are references) need
//! not know about expressions:
//!
//! - Arithmetic wraps, as rustc does with overflow checks off, and shift
//!   amounts are masked to the type's width ([`BinOp::Shl`]).
//! - Division and remainder are preceded by an [`TerminatorKind::Assert`]
//!   that the divisor is not zero and, for signed types, that the division
//!   is not `MIN / -1`; rustc checks these even in release builds.
//! - `&&` and `||` are control flow; reads of a local are copied into a
//!   temporary first, so later operands cannot change them.
//! - Values are read from [`Place`]s, a local and a path of fields and
//!   dereferences into it, either copied or moved ([`Operand`]); tuples,
//!   structs and enums are built whole ([`Rvalue::Aggregate`]), and
//!   references by borrowing a place ([`Rvalue::Ref`]). An enum's fields
//!   are those of one of its variants, a [`ProjectionElem::Downcast`]
//!   away, and which one it holds is its discriminant
//!   ([`Rvalue::Discriminant`]).
//! - `match`, `if let` and `let`-`else` test the parts of a place their
//!   patterns look at, in order, and bind once the whole pattern (and an
//!   arm's guard) matched. Method calls are calls with
//!   the receiver's auto-dereferences and auto-borrow explicit.
//! - A block's bindings die at its end ([`StatementKind::StorageDead`]).
//! - Values are dropped ([`TerminatorKind::Drop`]) where their scope
//!   ends, bindings at the end of their block in reverse order and
//!   temporaries at the end of their statement, and before a place is
//!   overwritten. [`drops`] removes the drops of values moved out and
//!   makes those of values that may have been conditional, on *drop
//!   flags*; code generation expands each into calls of `Drop::drop`.
//!
//! [`build`] turns a type-checked body into MIR, and two passes check it
//! as rustc's borrow checker does: [`init`], that every place is
//! initialized, and not moved out of, before it is read, and that
//! immutable locals are assigned once; and [`borrowck`], that borrows do
//! not conflict with other uses of what they borrow.

mod bitset;
pub mod borrowck;
mod build;
pub mod drops;
pub mod eval;
pub mod init;
mod interval;
pub mod layout;
mod lens;
pub mod lint;
pub mod mono;
mod pretty;
pub mod validate;
pub mod vtable;

use flux_arena::{Arena, Idx};
use flux_hir::{FunctionId, StructId};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_ty::{ClosureId, ClosureKind, GenericArgs, IntTy, Mutability, Signatures, Ty, TyKind};

/// The discriminants of the enums whose discriminants are not all
/// literals ([`flux_hir::Discriminant::Const`]), evaluated, as bits:
/// what [`discriminant_bits`] needs for them.
pub type Discriminants = flux_arena::ArenaMap<flux_hir::Struct, Box<[u128]>>;

/// The discriminant of variant `variant` of enum `strukt`, as the bits
/// [`TerminatorKind::SwitchInt`] compares: of the type
/// [`flux_ty::discriminant_ty`] gives, which [`Rvalue::Discriminant`]
/// has. `None` for one that is not a literal's and is not in
/// `discriminants`, whose evaluation failed or did not happen.
pub fn discriminant_bits(
    items: &flux_hir::ItemTree,
    discriminants: &Discriminants,
    strukt: StructId,
    variant: u32,
) -> Option<u128> {
    let Some(variants) = items.structs[strukt].variants.as_ref() else {
        return Some(0);
    };
    match variants[variant as usize].discriminant {
        flux_hir::Discriminant::Value(value) => {
            match Constant::int(value, flux_ty::discriminant_ty(items, strukt)) {
                Constant::Int { bits, .. } => Some(bits),
                _ => unreachable!("an integer constant"),
            }
        }
        flux_hir::Discriminant::Const { .. } => discriminants
            .get(strukt)
            .map(|values| values[variant as usize]),
    }
}

pub use build::{build, build_file, check_clean_bodies};
pub use lens::check_file;

pub type Local = Idx<LocalDecl>;
pub type BasicBlock = Idx<BasicBlockData>;

/// A function's MIR.
#[derive(Debug, Clone)]
pub struct Body {
    /// `_0` is the return value, then one local per parameter, then the
    /// user's bindings and temporaries.
    pub locals: Arena<LocalDecl>,
    /// Type-only locals for written local lifetime annotations. They have no
    /// storage or runtime operations; region checking relates their projections
    /// to the initializer and bindings, including bindings assigned later.
    pub region_annotations: Vec<(Local, Box<[flux_ty::regions::Region]>, TextRange)>,
    /// Covariant initializer-to-annotation and invariant annotation-to-binding
    /// relations. The latter keep later writes subject to the annotation.
    pub region_relations: Vec<(Place, Place, flux_ty::regions::Variance, TextRange)>,
    pub blocks: Arena<BasicBlockData>,
    pub param_count: usize,
    /// The bytes of each C string constant, without the terminating NUL;
    /// [`Constant::CStr`] indexes this.
    pub c_strings: Vec<Box<[u8]>>,
    /// `core`'s code, which the overflow lints leave alone.
    pub in_core: bool,
    /// Its arithmetic is checked for overflow during constant evaluation
    /// when its caller's is (`#[rustc_inherit_overflow_checks]`). Other
    /// functions' wrap, as flux compiles them with overflow checks off;
    /// constants' are always checked.
    pub inherits_overflow_checks: bool,
    /// The drops as built that may find a value, with where it was made
    /// and the type of the place dropped, before [`drops`] split or
    /// guarded them: what E0493 reports in a constant or `const fn`,
    /// which may not drop anything whose `drop` runs code, even part of
    /// it.
    pub live_drops: Vec<(TextRange, Ty)>,
    /// What [`init`] found, before drop elaboration added the tests of
    /// the variants enums hold to drop what they still hold, as rustc
    /// checks before elaborating drops.
    pub init_errors: Vec<flux_hir::Diagnostic>,
    /// A closure's body's: its kind and what it captures, through `_1`.
    pub upvars: Option<Upvars>,
    /// The borrows that capture places for closures, by block and
    /// statement: where the capture's use is in the closure, which rustc
    /// reports for some errors, and whether it assigns the place.
    pub capture_uses: Vec<(BasicBlock, usize, TextRange, bool)>,
    /// The function whose body (or closure in its body) this is: whose
    /// signature and bounds it has.
    pub owner: Option<FunctionId>,
    /// The values [`Constant::Promoted`] refers to.
    pub promoted: Vec<eval::Value>,
    /// Code that type checking should have ruled out, which building
    /// lowered to `unreachable`: what and where. Expected only where the
    /// program has errors; anywhere else, a bug in flux, which
    /// [`build_file`] reports.
    pub bugs: Vec<(TextRange, &'static str)>,
    /// Where the method's name starts in each method call, by the block
    /// the call ends: where rustc's `#[track_caller]` locations point.
    pub fn_spans: rustc_hash::FxHashMap<BasicBlock, u32>,
}

/// What a closure's body captures, for messages and for what it may
/// change: each captured place is a field of `self` (`_1`, through a
/// reference unless the closure is `FnOnce`), itself a reference to the
/// place unless captured by value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upvars {
    pub kind: ClosureKind,
    /// Each captured place as written: `x`, `s.a`.
    pub names: Vec<String>,
    /// Captured by reference.
    pub by_ref: Vec<bool>,
    /// Captured by value from a `mut` binding (or a place in one), so
    /// that the closure may change it.
    pub mutable: Vec<bool>,
    /// A parameter's written type has a reference, whose lifetime is
    /// the closure's own (higher-ranked), as in rustc: what it is
    /// given there may not outlive the call. Not where its signature
    /// follows from a bound, whose lifetimes it then has.
    pub annotated_refs: bool,
}

impl Upvars {
    /// The upvar `place` is in, and how many of its projections lead
    /// to it, if it is in one.
    pub fn of(&self, place: &Place) -> Option<(usize, usize)> {
        if place.local != Local::from_raw(1) {
            return None;
        }
        let p = &place.projection;
        let (field, len) = match (self.kind, p.first(), p.get(1)) {
            (ClosureKind::FnOnce, Some(&ProjectionElem::Field(i)), _) => (i as usize, 1),
            (_, Some(ProjectionElem::Deref), Some(&ProjectionElem::Field(i))) => (i as usize, 2),
            _ => return None,
        };
        let by_ref = *self.by_ref.get(field)?;
        if by_ref {
            (p.get(len) == Some(&ProjectionElem::Deref)).then_some((field, len + 1))
        } else {
            Some((field, len))
        }
    }
}

impl Body {
    pub const RETURN: Local = Local::from_raw(0);

    pub fn entry() -> BasicBlock {
        BasicBlock::from_raw(0)
    }

    /// A body without locals or blocks, of `owner`.
    pub(crate) fn empty(owner: Option<FunctionId>) -> Body {
        Body {
            locals: Arena::new(),
            region_annotations: Vec::new(),
            region_relations: Vec::new(),
            blocks: Arena::new(),
            param_count: 0,
            c_strings: Vec::new(),
            in_core: false,
            inherits_overflow_checks: false,
            live_drops: Vec::new(),
            init_errors: Vec::new(),
            upvars: None,
            capture_uses: Vec::new(),
            owner,
            promoted: Vec::new(),
            bugs: Vec::new(),
            fn_spans: Default::default(),
        }
    }

    /// The locals holding the parameters, in order.
    pub fn params(&self) -> impl Iterator<Item = Local> {
        (1..=self.param_count as u32).map(Local::from_raw)
    }

    pub fn is_param(&self, local: Local) -> bool {
        (1..=self.param_count).contains(&local.index())
    }

    pub fn place_ty(&self, place: &Place, sigs: &Signatures) -> Ty {
        let local_ty = PlaceTy::from(self.locals[place.local].ty);
        place
            .projection
            .iter()
            .fold(local_ty, |ty, &elem| ty.project(elem, sigs))
            .ty
    }

    pub fn operand_ty(&self, operand: &Operand, sigs: &Signatures) -> Ty {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_ty(place, sigs),
            Operand::Const(constant) => constant.ty(),
        }
    }
}

/// `place` as rustc writes it in messages: `s.inner.0`, `*r`, and `r.a`
/// for a field behind a reference.
pub fn place_name(
    body: &Body,
    items: &flux_hir::ItemTree,
    sigs: &Signatures,
    place: &Place,
) -> String {
    let mut text = match body.locals[place.local].kind {
        LocalKind::User { name, .. } => name.to_string(),
        LocalKind::Static(id) => items.consts[id]
            .name
            .map_or("_".to_owned(), |n| n.to_string()),
        _ => format!("_{}", place.local.index()),
    };
    let mut ty = PlaceTy::from(body.locals[place.local].ty);
    // In a closure, a captured place is named as it is outside.
    let mut skip = 0;
    if let Some(upvars) = &body.upvars
        && let Some((field, len)) = upvars.of(place)
    {
        text = upvars.names[field].clone();
        for &elem in &place.projection[..len] {
            ty = ty.project(elem, sigs);
        }
        skip = len;
    }
    for (i, &elem) in place.projection.iter().enumerate().skip(skip) {
        // A static is named by itself, not the address it is used through.
        if i == 0 && matches!(body.locals[place.local].kind, LocalKind::Static(_)) {
            ty = ty.project(elem, sigs);
            continue;
        }
        match elem {
            // A field or element of a reference's target is written as if
            // the reference were the struct or array: `r.a`, `s[_]`, and
            // through a reference to it: `r.a` for `(**r).a`.
            ProjectionElem::Deref | ProjectionElem::BoxDeref
                if matches!(
                    place.projection[i + 1..]
                        .iter()
                        .find(|e| !matches!(e, ProjectionElem::Deref | ProjectionElem::BoxDeref)),
                    Some(
                        ProjectionElem::Field(_)
                            | ProjectionElem::Index(_)
                            | ProjectionElem::ConstantIndex { .. }
                            | ProjectionElem::Subslice { .. }
                    )
                ) => {}
            ProjectionElem::Index(_) => text = format!("{text}[_]"),
            // As rustc writes a slice pattern's elements and rest.
            ProjectionElem::ConstantIndex { .. } | ProjectionElem::Subslice { .. } => {
                text = format!("{text}[..]")
            }
            ProjectionElem::Deref | ProjectionElem::BoxDeref => text = format!("*{text}"),
            // rustc names a variant's field as if it were the enum's:
            // `o.0` for `(o as Some).0`.
            ProjectionElem::Downcast(_) => {}
            ProjectionElem::Field(field) => match ty.ty.kind() {
                TyKind::Adt(adt, _) => {
                    let variant = items.structs[adt.strukt].variant(ty.variant.unwrap_or(0));
                    let name = variant.fields[field as usize].name;
                    text = format!("{text}.{name}");
                }
                _ => text = format!("{text}.{field}"),
            },
        }
        ty = ty.project(elem, sigs);
    }
    text
}

/// The type of a place, and the variant of it a [`ProjectionElem::Downcast`]
/// chose, whose fields its fields are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaceTy {
    pub ty: Ty,
    pub variant: Option<u32>,
}

impl From<Ty> for PlaceTy {
    fn from(ty: Ty) -> PlaceTy {
        PlaceTy { ty, variant: None }
    }
}

impl PlaceTy {
    /// The type of `elem` of a place of this type.
    pub fn project(self, elem: ProjectionElem, sigs: &Signatures) -> PlaceTy {
        let ty = match (elem, self.ty.kind()) {
            (ProjectionElem::Downcast(variant), _) => {
                return PlaceTy {
                    ty: self.ty,
                    variant: Some(variant),
                };
            }
            (ProjectionElem::Field(field), _) => sigs
                .variant_fields(self.ty, self.variant.unwrap_or(0))
                .and_then(|fields| fields.get(field as usize).copied())
                .unwrap_or(Ty::ERROR),
            (ProjectionElem::Deref, TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee)) => {
                pointee
            }
            (ProjectionElem::Deref, _) => Ty::ERROR,
            (ProjectionElem::BoxDeref, _) => flux_ty::box_pointee(self.ty).unwrap_or(Ty::ERROR),
            (
                ProjectionElem::Index(_) | ProjectionElem::ConstantIndex { .. },
                TyKind::Array(elem, _) | TyKind::Slice(elem),
            ) => elem,
            (ProjectionElem::Subslice { from, to, .. }, TyKind::Array(elem, _)) => {
                Ty::array_of(elem, Ty::usize_value(u64::from(to.saturating_sub(from))))
            }
            (ProjectionElem::Subslice { .. }, TyKind::Slice(_)) => self.ty,
            (
                ProjectionElem::Index(_)
                | ProjectionElem::ConstantIndex { .. }
                | ProjectionElem::Subslice { .. },
                _,
            ) => Ty::ERROR,
        };
        PlaceTy::from(ty)
    }
}

/// A local, or a part of one or of what it points to: `_1`, `_1.0.2`,
/// `(*_1).0`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Place {
    pub local: Local,
    /// Outermost first.
    pub projection: Box<[ProjectionElem]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProjectionElem {
    /// A field of a tuple or struct, or of the variant before it, by
    /// index.
    Field(u32),
    /// What a reference or raw pointer points to.
    Deref,
    /// What a `Box` holds: through its pointer, but owned as a field
    /// is, so moved out of and borrowed as the box is.
    BoxDeref,
    /// An enum as its variant with this index, which it holds: `(e as
    /// Some)`. Only its fields are places.
    Downcast(u32),
    /// The element of an array or slice at the index in this local, a
    /// `usize` checked against the length first: `_1[_2]`.
    Index(Local),
    /// The element `offset` from the start, or with `from_end` from the
    /// end (`offset` 1 is the last), of an array or slice at least
    /// `min_length` long: what a slice pattern's elements are, as
    /// rustc's. Elements at different constant indices are different
    /// places.
    ConstantIndex {
        offset: u32,
        min_length: u32,
        from_end: bool,
    },
    /// The elements `from..to` of an array, or with `from_end`,
    /// `from..len - to` of a slice: what `rest @ ..` binds.
    Subslice { from: u32, to: u32, from_end: bool },
}

impl Place {
    pub fn field(&self, index: u32) -> Place {
        self.project(ProjectionElem::Field(index))
    }

    pub fn downcast(&self, variant: u32) -> Place {
        self.project(ProjectionElem::Downcast(variant))
    }

    pub fn deref(&self) -> Place {
        self.project(ProjectionElem::Deref)
    }

    /// `*self`, of a value of type `ty`: a `Box`'s contents, or what a
    /// pointer points to.
    pub fn deref_of(&self, ty: Ty) -> Place {
        if flux_ty::box_pointee(ty).is_some() {
            self.project(ProjectionElem::BoxDeref)
        } else {
            self.project(ProjectionElem::Deref)
        }
    }

    pub fn index(&self, local: Local) -> Place {
        self.project(ProjectionElem::Index(local))
    }

    /// The locals the place's indices are in.
    pub fn index_locals(&self) -> impl Iterator<Item = Local> + '_ {
        self.projection.iter().filter_map(|elem| match elem {
            ProjectionElem::Index(local) => Some(*local),
            _ => None,
        })
    }

    pub fn project(&self, elem: ProjectionElem) -> Place {
        let mut projection = self.projection.to_vec();
        projection.push(elem);
        Place {
            local: self.local,
            projection: projection.into(),
        }
    }

    /// Whether the place is behind a pointer.
    pub fn has_deref(&self) -> bool {
        self.projection.contains(&ProjectionElem::Deref)
    }
}

impl From<Local> for Place {
    fn from(local: Local) -> Place {
        Place {
            local,
            projection: Box::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalDecl {
    pub ty: Ty,
    pub kind: LocalKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
    Return,
    /// A parameter whose pattern is not a single binding.
    Param,
    /// A binding, of a `let` or a parameter.
    User {
        name: Symbol,
        mutable: bool,
    },
    Temp,
    /// A temporary holding the address of a static, which its uses go
    /// through: `(*_3).a` is the static's field `a`.
    Static(flux_hir::ConstId),
    /// A temporary holding the reference an overloaded index
    /// (`Index::index`) or dereference (`Deref::deref`) of a `ty`
    /// returns, which the place it denotes is behind.
    Overloaded {
        index: bool,
        ty: Ty,
    },
}

impl LocalKind {
    /// A temporary, of the builder's own or holding an overloaded
    /// place's reference.
    pub fn is_temp(self) -> bool {
        matches!(self, LocalKind::Temp | LocalKind::Overloaded { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlockData {
    pub statements: Vec<Statement>,
    pub terminator: Terminator,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statement {
    pub kind: StatementKind,
    /// The source of the expression this came from.
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatementKind {
    Assign(Place, Rvalue),
    /// The local's scope ends: it can no longer be used or borrowed.
    StorageDead(Local),
    /// `let _ = place;`: names the place without reading it, which keeps
    /// the references it goes through alive.
    PlaceMention(Place),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    /// An integer-to-integer or `bool`-to-integer cast: truncates, or
    /// extends by the source's signedness. Also `CStr::as_ptr`, a
    /// `&CStr` as the `*const c_char` it is.
    Cast(Operand, Ty),
    /// A tuple or struct from its fields, in declaration order.
    Aggregate(AggregateKind, Vec<Operand>),
    /// The discriminant of the enum in `place`, as an `isize`: the value
    /// of the variant it holds.
    Discriminant(Place),
    /// `&place` or `&mut place`. A *two-phase* `&mut`, made for a method
    /// call's receiver, acts as a shared borrow until the call, so that
    /// the arguments may still read the place: `s.set(s.get())`.
    Ref {
        mutability: Mutability,
        place: Place,
        two_phase: bool,
    },
    /// The length of the array or slice at `place`, a `usize`. With
    /// `borrows`, for a call of `len`, the place is borrowed as `&self`
    /// is, so it conflicts with mutable borrows of it; without, for a
    /// bounds check, only the pointer a slice is behind is read.
    Len {
        place: Place,
        borrows: bool,
    },
    /// `[value; count]`: an array of `count` copies of `value`, `count`
    /// a constant of type `usize` ([`TyKind::Value`] once
    /// monomorphized).
    Repeat(Operand, Ty),
    /// A reference or raw pointer to an array, as one to a slice of its
    /// elements, of type `ty`: rustc's unsizing coercion.
    Unsize(Operand, Ty),
    /// The elements `start..end` of the slice a reference points to, as
    /// a reference of the same kind, unchecked: what `core`'s
    /// `subslice` intrinsics become once monomorphized, so only code
    /// generation sees it.
    Subslice {
        slice: Operand,
        start: Operand,
        end: Operand,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateKind {
    Tuple,
    /// An array of elements of this type, from its elements.
    Array(Ty),
    Struct(StructId),
    /// Variant `index` of an enum, from its fields.
    Variant(StructId, u32),
    /// A closure, from what it captures.
    Closure,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    /// Reads a value of a `Copy` type.
    Copy(Place),
    /// Reads a value of another type, leaving the place uninitialized.
    Move(Place),
    Const(Constant),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Constant {
    /// An integer as its two's complement bits, truncated to `ty`'s width.
    Int {
        bits: u128,
        ty: IntTy,
    },
    Bool(bool),
    /// A `char`, held as its `u32` value.
    Char(char),
    Unit,
    /// The value of a function item's type ([`TyKind::FnDef`]), which
    /// has no size.
    FnItem(Ty),
    /// A `c"..."` literal: a pointer to NUL-terminated static bytes, the
    /// body's `c_strings[index]`.
    CStr(u32),
    /// The value of a constant item, of type `ty`, with generic
    /// arguments for its owner's parameters (`Self` first, for a
    /// trait's): which value it is follows once they are known.
    Item {
        id: flux_hir::ConstId,
        args: GenericArgs,
        ty: Ty,
    },
    /// A pointer to a function, `function` with generic arguments
    /// `args`, of function pointer type `ty`: a function item coerced to
    /// one.
    Fn {
        function: FunctionId,
        args: GenericArgs,
        ty: Ty,
    },
    /// The address of static `id`: of type `ty`, `&T` for a `static`
    /// and `*mut T` for a `static mut`, as rustc's MIR has it.
    Static {
        id: flux_hir::ConstId,
        ty: Ty,
    },
    /// A reference, of type `ty`, to the body's `promoted[index]`: a
    /// constant expression borrowed (`&5`, `&[1, 2]`), which rustc
    /// *promotes* to a value in static memory that lives forever.
    Promoted {
        index: u32,
        ty: Ty,
    },
    /// A constant of the type system, of type `ty`: a const parameter
    /// (`N`), or an array's length that depends on one. Substituting
    /// generic arguments makes it an `Int`, `Bool` or `Char`
    /// ([`Constant::of_ty_const`]).
    TyConst {
        value: Ty,
        ty: Ty,
    },
}

impl Constant {
    pub fn ty(self) -> Ty {
        match self {
            Constant::Int { ty, .. } => Ty::int(ty),
            Constant::Bool(_) => Ty::BOOL,
            Constant::Char(_) => Ty::CHAR,
            Constant::Unit => Ty::UNIT,
            Constant::FnItem(ty) => ty,
            Constant::CStr(_) => flux_ty::c_str_ref(),
            Constant::Item { ty, .. }
            | Constant::Static { ty, .. }
            | Constant::Fn { ty, .. }
            | Constant::Promoted { ty, .. }
            | Constant::TyConst { ty, .. } => ty,
        }
    }

    /// `value` as a constant of type `ty`.
    /// An integer constant's bits.
    pub fn int_bits(self) -> Option<u128> {
        match self {
            Constant::Int { bits, .. } => Some(bits),
            _ => None,
        }
    }

    /// The constant `value` of the type system, of type `ty`: its value
    /// if it is known.
    pub fn of_ty_const(value: Ty, ty: Ty) -> Constant {
        let Some(v) = value.as_value() else {
            return Constant::TyConst { value, ty };
        };
        match v.ty.kind() {
            TyKind::Int(int) => Constant::Int {
                bits: v.bits(),
                ty: int,
            },
            TyKind::Bool => Constant::Bool(v.bits() != 0),
            TyKind::Char => Constant::Char(char::from_u32(v.bits() as u32).unwrap_or('\0')),
            _ => Constant::TyConst { value, ty },
        }
    }

    pub fn int(value: i128, ty: IntTy) -> Constant {
        let bits = ty.bits();
        let mask = if bits == 128 {
            u128::MAX
        } else {
            (1u128 << bits) - 1
        };
        Constant::Int {
            bits: (value as u128) & mask,
            ty,
        }
    }
}

/// Binary operators on integers, and on `bool` for comparisons and the
/// bitwise operators. Both operands have the same type except for shifts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    /// Wrapping.
    Add,
    /// Wrapping.
    Sub,
    /// Wrapping.
    Mul,
    /// Truncating; the divisor was checked.
    Div,
    /// The divisor was checked.
    Rem,
    /// The shift amount is masked to the left operand's width.
    Shl,
    /// Arithmetic for signed types, logical for unsigned; masked like
    /// `Shl`.
    Shr,
    BitAnd,
    BitOr,
    BitXor,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnOp {
    /// Wrapping negation.
    Neg,
    /// Logical not for `bool`, bitwise not for integers.
    Not,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Terminator {
    pub kind: TerminatorKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminatorKind {
    Goto(BasicBlock),
    /// Jumps to the target whose value equals `discr`, else `otherwise`.
    SwitchInt {
        discr: Operand,
        targets: Vec<(u128, BasicBlock)>,
        otherwise: BasicBlock,
        /// `Some(e)` if `discr` is enum `e`'s discriminant and each
        /// target's value is the index of a variant of `e`, standing for
        /// its discriminant ([`discriminant_bits`]): discriminants other
        /// than literals are known only once constants are evaluated,
        /// which may need this body.
        variants: Option<StructId>,
    },
    /// Calls `func`, stores its result in `dest` and continues at
    /// `target`; `target` is `None` when `func` returns `!`.
    Call {
        func: FunctionId,
        /// `func`'s generic arguments, in terms of the caller's.
        generic_args: GenericArgs,
        args: Vec<Operand>,
        dest: Place,
        target: Option<BasicBlock>,
    },
    /// Calls the function `callee`, a function pointer, points to, as
    /// [`TerminatorKind::Call`] does.
    CallIndirect {
        callee: Operand,
        args: Vec<Operand>,
        dest: Place,
        target: Option<BasicBlock>,
    },
    /// Drops the value in `place`, if its type needs it: runs its
    /// `Drop` impl, then drops its fields in order. After [`drops`],
    /// the place is initialized here.
    Drop {
        place: Place,
        target: BasicBlock,
    },
    /// Continues at `target` if `cond` is `expected`; otherwise panics,
    /// which aborts the program.
    Assert {
        cond: Operand,
        expected: bool,
        msg: AssertMessage,
        target: BasicBlock,
    },
    Return,
    /// Never reached: code after something that diverges.
    Unreachable,
}

/// Why an assertion fails, with the operands of the operation it
/// guards, for constant evaluation's messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssertMessage {
    DivisionByZero(Operand),
    RemainderByZero(Operand),
    DivisionOverflow(Operand, Operand),
    RemainderOverflow(Operand, Operand),
    /// An index past the end of an array or slice: the length, then the
    /// index.
    BoundsCheck {
        len: Operand,
        index: Operand,
    },
}

impl AssertMessage {
    /// The `#[lang]` name of `core`'s function that panics with the
    /// message, which code generation calls where the check fails.
    pub fn lang_fn(&self) -> &'static str {
        match self {
            AssertMessage::DivisionByZero(_) => "panic_const_div_by_zero",
            AssertMessage::RemainderByZero(_) => "panic_const_rem_by_zero",
            AssertMessage::DivisionOverflow(..) => "panic_const_div_overflow",
            AssertMessage::RemainderOverflow(..) => "panic_const_rem_overflow",
            AssertMessage::BoundsCheck { .. } => "panic_bounds_check",
        }
    }

    /// rustc's panic message.
    pub fn text(&self) -> &'static str {
        match self {
            AssertMessage::DivisionByZero(_) => "attempt to divide by zero",
            AssertMessage::RemainderByZero(_) => {
                "attempt to calculate the remainder with a divisor of zero"
            }
            AssertMessage::DivisionOverflow(..) => "attempt to divide with overflow",
            AssertMessage::RemainderOverflow(..) => {
                "attempt to calculate the remainder with overflow"
            }
            AssertMessage::BoundsCheck { .. } => "index out of bounds",
        }
    }
}

impl TerminatorKind {
    pub fn successors(&self) -> Vec<BasicBlock> {
        match self {
            TerminatorKind::Goto(target) => vec![*target],
            TerminatorKind::SwitchInt {
                targets, otherwise, ..
            } => targets
                .iter()
                .map(|&(_, bb)| bb)
                .chain(Some(*otherwise))
                .collect(),
            TerminatorKind::Call { target, .. } | TerminatorKind::CallIndirect { target, .. } => {
                target.iter().copied().collect()
            }
            TerminatorKind::Assert { target, .. } | TerminatorKind::Drop { target, .. } => {
                vec![*target]
            }
            TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}

/// The MIR of a file: one body per function with a body.
#[derive(Debug)]
pub struct FileMir {
    pub bodies: flux_arena::ArenaMap<flux_hir::Function, Body>,
    /// Each closure's body, as the method of each `Fn`-family trait it
    /// implements: its own kind's, built from its expression and
    /// checked, and *shims* for the others, which call it (`call_once`
    /// of an `Fn` closure borrows it, calls `call`, then drops it). In
    /// terms of its owner's generic parameters.
    pub closures: std::collections::HashMap<(ClosureId, ClosureKind), Body>,
    /// Each constant's value, as MIR.
    pub consts: flux_arena::ArenaMap<flux_hir::Const, Body>,
    /// The values of the constants without generic parameters, and of
    /// the instances of others that code without any names.
    pub const_values: eval::ConstValues,
    /// The discriminants of the enums whose discriminants are not all
    /// literals, once evaluated.
    pub discriminants: Discriminants,
    pub diagnostics: Vec<flux_hir::Diagnostic>,
    /// [`Body::bugs`] of every body, also among `diagnostics`: code
    /// type checking should have ruled out, a bug in flux. Debug builds
    /// panic on them instead.
    pub bugs: Vec<flux_hir::Diagnostic>,
}

impl FileMir {
    /// Every body in a textual form like rustc's `-Zdump-mir`. Used by
    /// snapshot tests and `flux mir`.
    pub fn debug_dump(&self, hir: &flux_hir::Hir) -> String {
        pretty::file(self, hir)
    }

    /// The MIR of `instance`, and the generic arguments to substitute in
    /// it: a function's own, or for a `Fn`-family method on a closure,
    /// the closure's body or shim, with its owner's arguments.
    pub fn instance_body(
        &self,
        items: &flux_hir::ItemTree,
        instance: flux_ty::Instance,
    ) -> Option<(&Body, GenericArgs)> {
        if let Some(body) = self.bodies.get(instance.function) {
            return Some((body, instance.args));
        }
        let (id, args) = match instance.args.first()?.kind() {
            TyKind::Closure(id, args) => (id, args),
            _ => return None,
        };
        let mode = fn_method_kind(items, instance.function)?;
        let body = self.closures.get(&(id, mode))?;
        Some((body, flux_ty::ClosureArgs::split(args).parent))
    }
}

/// The `Fn`-family trait `function` is the method of, if it is `call`,
/// `call_mut` or `call_once`.
pub fn fn_method_kind(items: &flux_hir::ItemTree, function: FunctionId) -> Option<ClosureKind> {
    match items.functions[function].owner? {
        flux_hir::Owner::Trait(t) => flux_ty::fn_trait_kind(items, t),
        flux_hir::Owner::Impl(_) => None,
    }
}

impl Body {
    /// The functions the body calls, or names as values: function items
    /// (`.all(u8::is_ascii_digit)`), in its locals' types and its calls'
    /// generic arguments, which a shim may call once monomorphized, and
    /// functions coerced to pointers.
    pub fn callees(&self) -> Vec<FunctionId> {
        let mut found = Vec::new();
        let mut seen = rustc_hash::FxHashSet::default();
        let mut stack: Vec<Ty> = self.locals.values().map(|l| l.ty).collect();
        for data in self.blocks.values() {
            if let TerminatorKind::Call {
                func, generic_args, ..
            } = &data.terminator.kind
            {
                found.push(*func);
                stack.extend(generic_args.iter().copied());
            }
            let mut operand = |op: &Operand| match op {
                Operand::Const(Constant::Fn { function, args, .. }) => {
                    found.push(*function);
                    stack.extend(args.iter().copied());
                }
                Operand::Const(Constant::FnItem(ty)) => stack.push(*ty),
                _ => {}
            };
            for statement in &data.statements {
                if let StatementKind::Assign(_, rvalue) = &statement.kind {
                    rvalue.operands().into_iter().for_each(&mut operand);
                }
            }
            data.terminator
                .kind
                .operands()
                .into_iter()
                .for_each(&mut operand);
        }
        while let Some(ty) = stack.pop() {
            if !seen.insert(ty) {
                continue;
            }
            if let TyKind::FnDef(def) = ty.kind() {
                found.push(def.function);
            }
            stack.extend(ty.children());
        }
        found
    }

    /// The methods the body's trait objects' vtables may hold: those of
    /// the traits (and supertraits) of what it unsizes values to.
    pub fn object_methods(&self, items: &flux_hir::ItemTree) -> Vec<FunctionId> {
        let mut traits = Vec::new();
        for data in self.blocks.values() {
            for statement in &data.statements {
                if let StatementKind::Assign(_, Rvalue::Unsize(_, to)) = &statement.kind
                    && let TyKind::Ref(_, p) | TyKind::RawPtr(_, p) = to.kind()
                    && let TyKind::Dynamic(d) = p.kind()
                    && !traits.contains(&d.trait_)
                {
                    traits.push(d.trait_);
                }
            }
        }
        // And what drops a value in place, which a vtable's first word
        // calls.
        let drop = (!traits.is_empty())
            .then(|| flux_ty::objects::drop_in_place(items))
            .flatten();
        traits
            .into_iter()
            .flat_map(|t| flux_ty::objects::supertraits_of(items, t))
            .flat_map(|t| items.traits[t].functions.iter().copied())
            .chain(drop)
            .collect()
    }

    /// The constant items the body uses, with their generic arguments.
    pub fn const_items(&self) -> Vec<flux_ty::ConstInstance> {
        let mut found = Vec::new();
        let mut operand = |op: &Operand| {
            if let Operand::Const(Constant::Item { id, args, .. }) = op {
                found.push(flux_ty::ConstInstance { konst: *id, args });
            }
        };
        for data in self.blocks.values() {
            for statement in &data.statements {
                if let StatementKind::Assign(_, rvalue) = &statement.kind {
                    rvalue.operands().into_iter().for_each(&mut operand);
                }
            }
            data.terminator
                .kind
                .operands()
                .into_iter()
                .for_each(&mut operand);
        }
        found
    }
}

impl Rvalue {
    /// The operands it reads.
    pub fn operands(&self) -> Vec<&Operand> {
        match self {
            Rvalue::Use(op)
            | Rvalue::UnaryOp(_, op)
            | Rvalue::Cast(op, _)
            | Rvalue::Repeat(op, _)
            | Rvalue::Unsize(op, _) => vec![op],
            Rvalue::BinaryOp(_, l, r) => vec![l, r],
            Rvalue::Subslice { slice, start, end } => vec![slice, start, end],
            Rvalue::Aggregate(_, ops) => ops.iter().collect(),
            Rvalue::Ref { .. } | Rvalue::Discriminant(_) | Rvalue::Len { .. } => Vec::new(),
        }
    }
}

impl TerminatorKind {
    /// The operands it reads.
    pub fn operands(&self) -> Vec<&Operand> {
        match self {
            TerminatorKind::SwitchInt { discr, .. } => vec![discr],
            TerminatorKind::Call { args, .. } => args.iter().collect(),
            TerminatorKind::CallIndirect { callee, args, .. } => {
                std::iter::once(callee).chain(args).collect()
            }
            TerminatorKind::Assert { cond, .. } => vec![cond],
            TerminatorKind::Goto(_)
            | TerminatorKind::Drop { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}
