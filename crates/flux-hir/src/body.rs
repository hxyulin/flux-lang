//! Function bodies: expressions, patterns and bindings in arenas.

use flux_arena::{Arena, ArenaMap, Idx};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use flux_syntax::ast::{BinaryOp, UnaryOp};

use crate::{ConstId, FunctionId, StructId, TraitId, TypeRef};

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
    /// The generic arguments written in a `Path`, `Record` or
    /// `MethodCall` expression, for those that have any or go through
    /// `Self`.
    pub generic_args: ArenaMap<Expr, PathGenerics>,
    /// The same for struct and variant patterns, `E::<T>::A(x)`.
    pub pat_generic_args: ArenaMap<Pat, PathGenerics>,
    /// The bindings in later alternatives of or-patterns that bind by
    /// reference where the first binds by value, or the reverse
    /// (reported): how each binds, `ref` (`Some(false)`), `ref mut`
    /// (`Some(true)`) or by value, for the types rustc reports.
    pub alt_by_ref: ArenaMap<Pat, Option<bool>>,
    /// The function's block.
    pub root: ExprId,
    /// The module of each block with items, where the names in it are
    /// resolved.
    pub block_modules: ArenaMap<Expr, crate::ModuleId>,
    /// The blocks written `const { .. }`: inline constants, evaluated as
    /// constants are (rustc's), which see no locals around them.
    pub const_blocks: ArenaMap<Expr, ()>,
}

/// Where each node of a [`Body`] came from. Desugared nodes map to the
/// construct they came from: the `loop` of a `while` maps to the `while`.
#[derive(Debug, Default)]
pub struct BodySourceMap {
    pub exprs: ArenaMap<Expr, TextRange>,
    pub pats: ArenaMap<Pat, TextRange>,
    pub types: ArenaMap<TypeRef, TextRange>,
    /// The field names of a `Field` expression (one) or a `Record`
    /// expression (one per field, in order), and the method name of a
    /// `MethodCall`.
    pub expr_fields: ArenaMap<Expr, Box<[TextRange]>>,
    /// Each field of a `Record` pattern: its name, and the whole field
    /// (`a: b`).
    pub pat_fields: ArenaMap<Pat, Box<[(TextRange, TextRange)]>>,
    /// The operator of a `Binary` expression, the `?` of a `Try`, and
    /// an `Index` expression from its `[` on.
    pub ops: ArenaMap<Expr, TextRange>,
    /// The path of a `Record` expression.
    pub paths: ArenaMap<Expr, TextRange>,
    /// The last segment of a `Path` expression with more than one: the
    /// name of an associated item.
    pub segments: ArenaMap<Expr, TextRange>,
    /// A parenthesized expression's own range, without the parentheses,
    /// which `exprs` includes as rustc's spans do: for what rustc reports
    /// at the expression itself (an out-of-range literal).
    pub unparenthesized: ArenaMap<Expr, TextRange>,
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
    /// `if c { .. } else { .. }`. The condition may be or contain
    /// [`Expr::Let`]s, joined by `&&`, whose bindings the `then` branch
    /// sees.
    If {
        condition: ExprId,
        then_branch: ExprId,
        else_branch: Option<ExprId>,
    },
    /// `let pat = expr` in the condition of an `if` or `while`, alone or
    /// in a chain of `&&`: true if the value matches the pattern.
    Let {
        pat: PatId,
        expr: ExprId,
    },
    /// `match scrutinee { arms }`
    Match {
        scrutinee: ExprId,
        arms: Box<[MatchArm]>,
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
    /// `expr?`: the value of an `Option`'s `Some` or a `Result`'s `Ok`,
    /// or else a return of the `None`, or of the `Err` converted with
    /// `From`. rustc desugars it through the unstable `Try` trait, which
    /// only these implement on stable Rust (and `ControlFlow` and
    /// `Poll`, which flux lacks).
    Try {
        expr: ExprId,
    },
    /// `expr.name`, also `tuple.0`; resolved during type checking.
    Field {
        expr: ExprId,
        name: Symbol,
    },
    /// `&expr` or `&mut expr`.
    Ref {
        mutable: bool,
        expr: ExprId,
    },
    /// `&raw const place` or `&raw mut place`: a raw pointer to a place,
    /// made without a reference.
    RawRef {
        mutable: bool,
        expr: ExprId,
    },
    /// `[a, b, c]`.
    Array(Box<[ExprId]>),
    /// `[value; count]`.
    Repeat {
        value: ExprId,
        count: crate::ArrayLen,
    },
    /// `base[index]`, of an array or slice.
    Index {
        base: ExprId,
        index: ExprId,
    },
    /// `|a, b: i32| body`, `move |x| -> T { .. }`: a function of its
    /// own whose body is part of this one's, and which may use (capture)
    /// the bindings in scope where it is written. `return` in its body
    /// returns from it.
    Closure {
        params: Box<[PatId]>,
        /// Each parameter's type, if written.
        param_types: Box<[Option<TypeRefId>]>,
        ret: Option<TypeRefId>,
        body: ExprId,
        is_move: bool,
    },
    /// `S { a: 1, b, ..base }` or `E::V { a: 1 }`, in source order.
    Record {
        strukt: StructId,
        /// The variant of an enum; 0 for a struct.
        variant: u32,
        fields: Box<[(Symbol, ExprId)]>,
        spread: Option<ExprId>,
    },
}

/// `pat if guard => body`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pat: PatId,
    pub guard: Option<ExprId>,
    pub body: ExprId,
}

/// The generic arguments of an expression's path, by what they are for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathGenerics {
    /// For the type an associated item is of: `S::<A>::f`.
    pub ty: Option<SegmentArgs>,
    /// For the item named: `f::<A>`, `S::<A>(x)`, `S::<A> { .. }`, or
    /// the method of `x.f::<A>()`.
    pub item: Option<SegmentArgs>,
    /// The type is `Self`, whose generic arguments are the `impl`
    /// block's parameters: `Self::f`, `Self(x)`, `Self { .. }`.
    pub self_type: bool,
    /// The struct or enum is named by a type alias, whose type, with
    /// the arguments written for the alias, gives its generic
    /// arguments: `A { .. }`, `A::V(x)` for `type A = E<u8>;`.
    pub alias: Option<TypeRefId>,
}

/// The generic arguments after one segment of a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SegmentArgs {
    /// The segment's name.
    pub segment: TextRange,
    /// From `<` to `>`.
    pub range: TextRange,
    pub types: Box<[TypeRefId]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoopSource {
    Loop,
    /// `while cond { body }`, lowered as
    /// `loop { if cond { body } else { break } }`.
    While,
    /// `for pat in head { body }`: the loop of
    /// `match IntoIterator::into_iter(head) { mut iter => loop { match
    /// Iterator::next(&mut iter) { None => break, Some(pat) => body } } }`
    /// ([`Body::for_loop_match`]).
    For,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Literal {
    /// An integer, with its suffix (`u8` in `1u8`) if any.
    Int {
        value: u128,
        suffix: Option<Symbol>,
    },
    Bool(bool),
    /// `'a'`: a `char`.
    Char(char),
    /// `"..."` or `r"..."`: the text, escapes decoded.
    Str(Box<str>),
    /// `b"..."` or `br"..."`: the bytes, escapes decoded. (A byte
    /// literal, `b'a'`, is an [`Literal::Int`] with the suffix `u8`.)
    ByteStr(Box<[u8]>),
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
    /// An enum's variant: a tuple variant's constructor or a unit
    /// variant's value.
    Variant(StructId, u32),
    /// A constant, free or associated.
    Const(ConstId),
    /// A static, `static X` or `static mut X`: a place, not a value.
    Static(ConstId),
    /// A const generic parameter, `N` in `fn f<const N: usize>`, by name.
    ConstParam(Symbol),
    /// An associated item that type checking finds from a type's inherent
    /// and trait items: `T::f`, `S::f` for a trait's `f`, `Self::f` in a
    /// trait, `<T as Tr>::f` (with `trait_`), or `Tr::f` (without `ty`,
    /// whose `Self` is inferred). The trait's generic arguments, if
    /// written, are the path's `ty` arguments.
    Assoc {
        ty: Option<TypeRefId>,
        trait_: Option<TraitId>,
        name: Symbol,
    },
    /// Unresolved; already reported.
    Err,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let {
        pat: PatId,
        ty: Option<TypeRefId>,
        init: Option<ExprId>,
        /// `let pat = init else { .. };`: the block, which must diverge,
        /// run if `init` does not match `pat`.
        else_branch: Option<ExprId>,
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
    /// `x`, `mut x`, `ref x` or `ref mut x`, and with a pattern the value
    /// must match too, `x @ pat`.
    Bind {
        binding: BindingId,
        sub: Option<PatId>,
    },
    /// `(a, b)`; `rest` is the position of `..`, as in `(a, .., z)`.
    Tuple {
        pats: Box<[PatId]>,
        rest: Option<u32>,
    },
    /// `P(a, b)` or `E::V(a, b)`, with `..` like a tuple pattern.
    TupleStruct {
        strukt: StructId,
        /// The variant of an enum; 0 for a struct.
        variant: u32,
        pats: Box<[PatId]>,
        rest: Option<u32>,
    },
    /// `S { a, b: x, .. }` or `E::V { a, .. }`.
    Record {
        strukt: StructId,
        variant: u32,
        fields: Box<[(Symbol, PatId)]>,
        rest: bool,
    },
    /// A unit struct, `U`, or a unit variant, `E::A`.
    Unit(StructId, u32),
    /// A value to compare with, as an expression: a literal (`1`, `-1`,
    /// `true`) or a path to a constant (`N`, `i32::MAX`, `Self::N`).
    Expr(ExprId),
    /// `a..=b`, `a..b`, `a..` or `..=b`, whose ends are literals or paths
    /// to constants.
    Range {
        start: Option<ExprId>,
        end: Option<ExprId>,
        inclusive: bool,
    },
    /// `a | b`, each alternative binding the same names.
    Or(Box<[PatId]>),
    /// `&pat` or `&mut pat`.
    Ref { pat: PatId, mutable: bool },
    /// `[a, b]`, `[first, .., last]` or `[x, rest @ ..]`: the patterns
    /// before `..`, what `..` is if there is one (a wildcard, or a binding
    /// of the elements it stands for, `rest @ ..`), and those after it.
    Slice {
        prefix: Box<[PatId]>,
        slice: Option<PatId>,
        suffix: Box<[PatId]>,
    },
}

impl Pat {
    /// The patterns directly inside this one.
    pub fn children(&self) -> Box<dyn Iterator<Item = PatId> + '_> {
        let (pats, fields): (&[PatId], &[(Symbol, PatId)]) = match self {
            Pat::Tuple { pats, .. } | Pat::TupleStruct { pats, .. } | Pat::Or(pats) => (pats, &[]),
            Pat::Slice {
                prefix,
                slice,
                suffix,
            } => {
                let all = prefix.iter().chain(slice).chain(suffix.iter()).copied();
                return Box::new(all.collect::<Vec<_>>().into_iter())
                    as Box<dyn Iterator<Item = PatId>>;
            }
            Pat::Record { fields, .. } => (&[], fields),
            Pat::Bind { sub: Some(sub), .. } | Pat::Ref { pat: sub, .. } => {
                (std::slice::from_ref(sub), &[])
            }
            Pat::Missing
            | Pat::Wild
            | Pat::Bind { sub: None, .. }
            | Pat::Unit(..)
            | Pat::Expr(_)
            | Pat::Range { .. } => (&[], &[]),
        };
        Box::new(pats.iter().copied().chain(fields.iter().map(|&(_, p)| p)))
    }
}

impl Body {
    /// The `match` of `Iterator::next` in a `for` loop's `loop`, and the
    /// loop's own pattern, inside its `Some`.
    pub fn for_loop_match(&self, loop_expr: ExprId) -> Option<(ExprId, PatId)> {
        let Expr::Loop {
            body,
            source: LoopSource::For,
        } = self.exprs[loop_expr]
        else {
            return None;
        };
        let Expr::Block { stmts, .. } = &self.exprs[body] else {
            return None;
        };
        let [Stmt::Expr { expr, .. }] = &stmts[..] else {
            return None;
        };
        let Expr::Match { arms, .. } = &self.exprs[*expr] else {
            return None;
        };
        let Pat::TupleStruct { pats, .. } = &self.pats[arms.get(1)?.pat] else {
            return None;
        };
        Some((*expr, *pats.first()?))
    }

    /// Every binding in `pat`, in source order, once: an or-pattern's
    /// alternatives bind the same ones.
    pub fn pat_bindings(&self, pat: PatId, out: &mut Vec<BindingId>) {
        match &self.pats[pat] {
            Pat::Bind { binding, .. } if !out.contains(binding) => out.push(*binding),
            Pat::Or(alternatives) => {
                if let Some(&first) = alternatives.first() {
                    self.pat_bindings(first, out);
                }
                return;
            }
            _ => {}
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
    /// `ref x` (`Some(false)`) or `ref mut x` (`Some(true)`): binds a
    /// reference to the matched place rather than its value. Matching a
    /// reference with a pattern that is not one may make a plain binding
    /// one too; type checking says which ([`flux_ty`]'s binding modes).
    pub by_ref: Option<bool>,
}
