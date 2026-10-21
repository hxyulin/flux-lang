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
//!   dereferences into it, either copied or moved ([`Operand`]); tuples
//!   and structs are built whole ([`Rvalue::Aggregate`]), and references
//!   by borrowing a place ([`Rvalue::Ref`]). Method calls are calls with
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

pub mod borrowck;
mod build;
pub mod drops;
pub mod eval;
pub mod init;
pub mod lint;
pub mod mono;
mod pretty;

use flux_arena::{Arena, Idx};
use flux_hir::{FunctionId, StructId};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_ty::{GenericArgs, IntTy, Mutability, Signatures, Ty, TyKind};

pub use build::{build, build_file, check_clean_bodies};

pub type Local = Idx<LocalDecl>;
pub type BasicBlock = Idx<BasicBlockData>;

/// A function's MIR.
#[derive(Debug, Clone)]
pub struct Body {
    /// `_0` is the return value, then one local per parameter, then the
    /// user's bindings and temporaries.
    pub locals: Arena<LocalDecl>,
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
}

impl Body {
    pub const RETURN: Local = Local::from_raw(0);

    pub fn entry() -> BasicBlock {
        BasicBlock::from_raw(0)
    }

    /// The locals holding the parameters, in order.
    pub fn params(&self) -> impl Iterator<Item = Local> {
        (1..=self.param_count as u32).map(Local::from_raw)
    }

    pub fn is_param(&self, local: Local) -> bool {
        (1..=self.param_count).contains(&local.index())
    }

    pub fn place_ty(&self, place: &Place, sigs: &Signatures) -> Ty {
        let local_ty = self.locals[place.local].ty;
        place
            .projection
            .iter()
            .fold(local_ty, |ty, &elem| projection_ty(ty, elem, sigs))
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
        _ => format!("_{}", place.local.index()),
    };
    let mut ty = body.locals[place.local].ty;
    for (i, &elem) in place.projection.iter().enumerate() {
        match elem {
            // A field of a reference's target is written as if the
            // reference were the struct: `r.a`.
            ProjectionElem::Deref
                if matches!(place.projection.get(i + 1), Some(ProjectionElem::Field(_))) => {}
            ProjectionElem::Deref => text = format!("*{text}"),
            ProjectionElem::Field(field) => match ty.kind() {
                TyKind::Adt(adt, _) => {
                    let name = items.structs[adt.strukt].fields[field as usize].name;
                    text = format!("{text}.{name}");
                }
                _ => text = format!("{text}.{field}"),
            },
        }
        ty = projection_ty(ty, elem, sigs);
    }
    text
}

/// The type of `elem` of a value of type `ty`.
pub fn projection_ty(ty: Ty, elem: ProjectionElem, sigs: &Signatures) -> Ty {
    match (elem, ty.kind()) {
        (ProjectionElem::Field(field), _) => sigs
            .fields(ty)
            .map_or(Ty::ERROR, |fields| fields[field as usize]),
        (ProjectionElem::Deref, TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee)) => pointee,
        (ProjectionElem::Deref, _) => Ty::ERROR,
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
    /// A field of a tuple or struct, by index.
    Field(u32),
    /// What a reference or raw pointer points to.
    Deref,
}

impl Place {
    pub fn field(&self, index: u32) -> Place {
        self.project(ProjectionElem::Field(index))
    }

    pub fn deref(&self) -> Place {
        self.project(ProjectionElem::Deref)
    }

    fn project(&self, elem: ProjectionElem) -> Place {
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
    /// extends by the source's signedness.
    Cast(Operand, Ty),
    /// A tuple or struct from its fields, in declaration order.
    Aggregate(AggregateKind, Vec<Operand>),
    /// `&place` or `&mut place`. A *two-phase* `&mut`, made for a method
    /// call's receiver, acts as a shared borrow until the call, so that
    /// the arguments may still read the place: `s.set(s.get())`.
    Ref {
        mutability: Mutability,
        place: Place,
        two_phase: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregateKind {
    Tuple,
    Struct(StructId),
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
    Unit,
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
}

impl Constant {
    pub fn ty(self) -> Ty {
        match self {
            Constant::Int { ty, .. } => Ty::int(ty),
            Constant::Bool(_) => Ty::BOOL,
            Constant::Unit => Ty::UNIT,
            Constant::CStr(_) => flux_ty::c_str_ref(),
            Constant::Item { ty, .. } => ty,
        }
    }

    /// `value` as a constant of type `ty`.
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
}

impl AssertMessage {
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
            TerminatorKind::Call { target, .. } => target.iter().copied().collect(),
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
    /// Each constant's value, as MIR.
    pub consts: flux_arena::ArenaMap<flux_hir::Const, Body>,
    /// The values of the constants without generic parameters, and of
    /// the instances of others that code without any names.
    pub const_values: eval::ConstValues,
    pub diagnostics: Vec<flux_hir::Diagnostic>,
}

impl FileMir {
    /// Every body in a textual form like rustc's `-Zdump-mir`. Used by
    /// snapshot tests and `flux mir`.
    pub fn debug_dump(&self, hir: &flux_hir::Hir) -> String {
        pretty::file(self, hir)
    }
}

impl Body {
    /// The functions the body calls.
    pub fn callees(&self) -> Vec<FunctionId> {
        self.blocks
            .values()
            .filter_map(|data| match &data.terminator.kind {
                TerminatorKind::Call { func, .. } => Some(*func),
                _ => None,
            })
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
            Rvalue::Use(op) | Rvalue::UnaryOp(_, op) | Rvalue::Cast(op, _) => vec![op],
            Rvalue::BinaryOp(_, l, r) => vec![l, r],
            Rvalue::Aggregate(_, ops) => ops.iter().collect(),
            Rvalue::Ref { .. } => Vec::new(),
        }
    }
}

impl TerminatorKind {
    /// The operands it reads.
    pub fn operands(&self) -> Vec<&Operand> {
        match self {
            TerminatorKind::SwitchInt { discr, .. } => vec![discr],
            TerminatorKind::Call { args, .. } => args.iter().collect(),
            TerminatorKind::Assert { cond, .. } => vec![cond],
            TerminatorKind::Goto(_)
            | TerminatorKind::Drop { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}
