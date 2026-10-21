//! Function bodies: expressions, patterns and bindings in arenas.

use flux_arena::{Arena, ArenaMap, Idx};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_syntax::ast::{BinaryOp, UnaryOp};

use crate::{FunctionId, TypeRef};

pub type ExprId = Idx<Expr>;
pub type PatId = Idx<Pat>;
pub type BindingId = Idx<Binding>;
pub type TypeRefId = Idx<TypeRef>;

/// One function's code.
#[derive(Debug)]
pub struct Body {
    pub exprs: Arena<Expr>,
    pub pats: Arena<Pat>,
    pub bindings: Arena<Binding>,
    pub types: Arena<TypeRef>,
    /// One pattern per parameter, in order.
    pub params: Box<[PatId]>,
    /// The function's block.
    pub root: ExprId,
}

/// Where each node of a [`Body`] came from. Desugared nodes map to the
/// construct they came from: the `loop` of a `while` maps to the `while`.
#[derive(Debug, Default)]
pub struct BodySourceMap {
    pub exprs: ArenaMap<Expr, TextRange>,
    pub pats: ArenaMap<Pat, TextRange>,
    pub types: ArenaMap<TypeRef, TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    /// Malformed or unsupported; already reported.
    Missing,
    Literal(Literal),
    /// A name, resolved.
    Path(Res),
    /// `()` or `(a, b)`.
    Tuple(Box<[ExprId]>),
    Block {
        stmts: Box<[Stmt]>,
        tail: Option<ExprId>,
        is_unsafe: bool,
    },
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    Loop {
        body: ExprId,
        source: LoopSource,
    },
    Break {
        value: Option<ExprId>,
        /// The `Loop` it leaves; `None` outside any loop (reported).
        target: Option<ExprId>,
    },
    Continue {
        target: Option<ExprId>,
    },
    Return {
        value: Option<ExprId>,
    },
    Call {
        callee: ExprId,
        args: Box<[ExprId]>,
    },
    /// `receiver.method(args)`; resolved during type checking.
    MethodCall {
        receiver: ExprId,
        method: Symbol,
        args: Box<[ExprId]>,
    },
    Unary {
        op: UnaryOp,
        operand: ExprId,
    },
    /// Arithmetic, comparisons, `&&`/`||`, and assignments, including
    /// compound ones (`BinaryOp::Assign`).
    Binary {
        op: BinaryOp,
        lhs: ExprId,
        rhs: ExprId,
    },
    Cast {
        expr: ExprId,
        ty: TypeRefId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopSource {
    Loop,
    /// `while cond { body }`, lowered as
    /// `loop { if cond { body } else { break } }`.
    While,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// An integer, with its suffix (`u8` in `1u8`) if any.
    Int {
        value: u128,
        suffix: Option<Symbol>,
    },
    Bool(bool),
    /// `c"..."`: the bytes, escapes decoded, without the terminating NUL.
    CStr(Box<[u8]>),
}

/// What a name refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
    Local(BindingId),
    Function(FunctionId),
    /// Unresolved; already reported.
    Err,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        pat: PatId,
        ty: Option<TypeRefId>,
        init: Option<ExprId>,
    },
    Expr {
        expr: ExprId,
        has_semicolon: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pat {
    /// Malformed or unsupported; already reported.
    Missing,
    /// `_`
    Wild,
    /// `x` or `mut x`.
    Bind(BindingId),
}

/// A local variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: Symbol,
    pub mutable: bool,
}
