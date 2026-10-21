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
//!
//! [`build`] turns a type-checked body into MIR and checks that every
//! local is initialized before it is read and that immutable locals are
//! assigned once ([`init`]), the part of borrow checking flux needs so far.

mod build;
pub mod init;
mod pretty;

use flux_arena::{Arena, Idx};
use flux_hir::FunctionId;
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_ty::{IntTy, Ty};

pub use build::{build, build_file};

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

    pub fn operand_ty(&self, operand: &Operand) -> Ty {
        match operand {
            Operand::Copy(local) => self.locals[*local].ty,
            Operand::Const(constant) => constant.ty(),
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
    Param,
    /// A `let` binding or a parameter's binding.
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
    Assign(Local, Rvalue),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rvalue {
    Use(Operand),
    BinaryOp(BinOp, Operand, Operand),
    UnaryOp(UnOp, Operand),
    /// An integer-to-integer or `bool`-to-integer cast: truncates, or
    /// extends by the source's signedness.
    Cast(Operand, Ty),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operand {
    Copy(Local),
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
}

impl Constant {
    pub fn ty(self) -> Ty {
        match self {
            Constant::Int { ty, .. } => Ty::int(ty),
            Constant::Bool(_) => Ty::BOOL,
            Constant::Unit => Ty::UNIT,
            Constant::CStr(_) => flux_ty::c_str_ref(),
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
        args: Vec<Operand>,
        dest: Local,
        target: Option<BasicBlock>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssertMessage {
    DivisionByZero,
    RemainderByZero,
    DivisionOverflow,
    RemainderOverflow,
}

impl AssertMessage {
    /// rustc's panic message.
    pub fn text(self) -> &'static str {
        match self {
            AssertMessage::DivisionByZero => "attempt to divide by zero",
            AssertMessage::RemainderByZero => {
                "attempt to calculate the remainder with a divisor of zero"
            }
            AssertMessage::DivisionOverflow => "attempt to divide with overflow",
            AssertMessage::RemainderOverflow => "attempt to calculate the remainder with overflow",
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
            TerminatorKind::Assert { target, .. } => vec![*target],
            TerminatorKind::Return | TerminatorKind::Unreachable => Vec::new(),
        }
    }
}

/// The MIR of a file: one body per function with a body.
#[derive(Debug)]
pub struct FileMir {
    pub bodies: flux_arena::ArenaMap<flux_hir::Function, Body>,
    pub diagnostics: Vec<flux_hir::Diagnostic>,
}

impl FileMir {
    /// Every body in a textual form like rustc's `-Zdump-mir`. Used by
    /// snapshot tests and `flux mir`.
    pub fn debug_dump(&self, hir: &flux_hir::FileHir) -> String {
        pretty::file(self, hir)
    }
}
