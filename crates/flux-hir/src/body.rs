//! Function bodies: expressions, patterns and bindings in arenas.

use flux_arena::{Arena, ArenaMap, Idx};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_syntax::ast::{BinaryOp, UnaryOp};

use crate::{FunctionId, StructId, TypeRef};

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
    /// The field names of a `Field` expression (one) or a `Record`
    /// expression (one per field, in order).
    pub expr_fields: ArenaMap<Expr, Box<[TextRange]>>,
    /// Each field of a `Record` pattern: its name, and the whole field
    /// (`a: b`).
    pub pat_fields: ArenaMap<Pat, Box<[(TextRange, TextRange)]>>,
    /// The operator of a `Binary` expression.
    pub ops: ArenaMap<Expr, TextRange>,
    /// The path of a `Record` expression.
    pub paths: ArenaMap<Expr, TextRange>,
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
    /// `expr.name`, also `tuple.0`; resolved during type checking.
    Field {
        expr: ExprId,
        name: Symbol,
    },
    /// `S { a: 1, b, ..base }`, in source order.
    Record {
        strukt: StructId,
        fields: Box<[(Symbol, ExprId)]>,
        spread: Option<ExprId>,
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
    /// A tuple struct's constructor or a unit struct's value.
    Struct(StructId),
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
    /// `(a, b)`; `rest` is the position of `..`, as in `(a, .., z)`.
    Tuple {
        pats: Box<[PatId]>,
        rest: Option<u32>,
    },
    /// `P(a, b)`, with `..` like a tuple pattern.
    TupleStruct {
        strukt: StructId,
        pats: Box<[PatId]>,
        rest: Option<u32>,
    },
    /// `S { a, b: x, .. }`
    Record {
        strukt: StructId,
        fields: Box<[(Symbol, PatId)]>,
        rest: bool,
    },
    /// A unit struct: `U`.
    Unit(StructId),
}

impl Pat {
    /// The patterns directly inside this one.
    pub fn children(&self) -> impl Iterator<Item = PatId> + '_ {
        let (pats, fields): (&[PatId], &[(Symbol, PatId)]) = match self {
            Pat::Tuple { pats, .. } | Pat::TupleStruct { pats, .. } => (pats, &[]),
            Pat::Record { fields, .. } => (&[], fields),
            Pat::Missing | Pat::Wild | Pat::Bind(_) | Pat::Unit(_) => (&[], &[]),
        };
        pats.iter().copied().chain(fields.iter().map(|&(_, p)| p))
    }
}

impl Body {
    /// Every binding in `pat`, in source order.
    pub fn pat_bindings(&self, pat: PatId, out: &mut Vec<BindingId>) {
        if let Pat::Bind(binding) = self.pats[pat] {
            out.push(binding);
        }
        for child in self.pats[pat].children() {
            self.pat_bindings(child, out);
        }
    }
}

/// A local variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    pub name: Symbol,
    pub mutable: bool,
}
