//! Hand-written accessors: children that share a type and are told apart
//! by position (`lhs` and `rhs`), operators, and the text of names and
//! literals.

use super::{AstNode, support};
use super::{
    BinExpr, BlockExpr, Expr, ForExpr, IfExpr, Impl, IndexExpr, Lifetime, Literal, Name, NameRef,
    Pat, PathSegment, PathType, PrefixExpr, RangeExpr, RangePat, Type, WhileExpr,
};
use crate::{SyntaxKind, SyntaxNode, SyntaxToken, T};

/// The children of type `N` before and after `op`. Without `op`, the
/// first child counts as before.
fn split_at<'t, N: AstNode<'t>>(
    node: SyntaxNode<'t>,
    op: Option<SyntaxToken<'t>>,
) -> (Option<N>, Option<N>) {
    let Some(op) = op else {
        return (support::child(node), None);
    };
    let split = op.text_range().start();
    let (mut before, mut after) = (None, None);
    for child in support::children::<N>(node) {
        if child.syntax().text_range().end() <= split {
            before = Some(child);
        } else if after.is_none() {
            after = Some(child);
        }
    }
    (before, after)
}

/// The first token directly inside `node`: the operator of a unary,
/// binary or range expression, whose operands are nodes.
fn first_token(node: SyntaxNode<'_>) -> Option<SyntaxToken<'_>> {
    support::direct_tokens(node).next()
}

impl<'t> Name<'t> {
    pub fn text(self) -> &'t str {
        first_token(self.syntax).map_or("", SyntaxToken::text)
    }
}

impl<'t> NameRef<'t> {
    pub fn text(self) -> &'t str {
        first_token(self.syntax).map_or("", SyntaxToken::text)
    }
}

impl<'t> Lifetime<'t> {
    /// The lifetime with its quote: `'a`.
    pub fn text(self) -> &'t str {
        first_token(self.syntax).map_or("", SyntaxToken::text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiteralKind {
    Int,
    Float,
    Char,
    Byte,
    String,
    ByteString,
    CString,
    Bool(bool),
}

impl<'t> Literal<'t> {
    pub fn value(self) -> Option<SyntaxToken<'t>> {
        first_token(self.syntax)
    }

    pub fn kind(self) -> Option<LiteralKind> {
        let kind = match self.value()?.kind() {
            SyntaxKind::INT_NUMBER => LiteralKind::Int,
            SyntaxKind::FLOAT_NUMBER => LiteralKind::Float,
            SyntaxKind::CHAR => LiteralKind::Char,
            SyntaxKind::BYTE => LiteralKind::Byte,
            SyntaxKind::STRING => LiteralKind::String,
            SyntaxKind::BYTE_STRING => LiteralKind::ByteString,
            SyntaxKind::C_STRING => LiteralKind::CString,
            T![true] => LiteralKind::Bool(true),
            T![false] => LiteralKind::Bool(false),
            _ => return None,
        };
        Some(kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Logic(LogicOp),
    Arith(ArithOp),
    Cmp(CmpOp),
    /// `=`, or a compound assignment such as `+=`.
    Assign(Option<ArithOp>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogicOp {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Shr,
    BitXor,
    BitOr,
    BitAnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

impl<'t> BinExpr<'t> {
    pub fn op(self) -> Option<SyntaxToken<'t>> {
        first_token(self.syntax)
    }

    pub fn op_kind(self) -> Option<BinaryOp> {
        use ArithOp::*;
        use BinaryOp::*;
        let op = match self.op()?.kind() {
            T![||] => Logic(LogicOp::Or),
            T![&&] => Logic(LogicOp::And),
            T![==] => Cmp(CmpOp::Eq),
            T![!=] => Cmp(CmpOp::Ne),
            T![<] => Cmp(CmpOp::Lt),
            T![<=] => Cmp(CmpOp::Le),
            T![>] => Cmp(CmpOp::Gt),
            T![>=] => Cmp(CmpOp::Ge),
            T![+] => Arith(Add),
            T![-] => Arith(Sub),
            T![*] => Arith(Mul),
            T![/] => Arith(Div),
            T![%] => Arith(Rem),
            T![<<] => Arith(Shl),
            T![>>] => Arith(Shr),
            T![^] => Arith(BitXor),
            T![|] => Arith(BitOr),
            T![&] => Arith(BitAnd),
            T![=] => Assign(None),
            T![+=] => Assign(Some(Add)),
            T![-=] => Assign(Some(Sub)),
            T![*=] => Assign(Some(Mul)),
            T![/=] => Assign(Some(Div)),
            T![%=] => Assign(Some(Rem)),
            T![<<=] => Assign(Some(Shl)),
            T![>>=] => Assign(Some(Shr)),
            T![^=] => Assign(Some(BitXor)),
            T![|=] => Assign(Some(BitOr)),
            T![&=] => Assign(Some(BitAnd)),
            _ => return None,
        };
        Some(op)
    }

    pub fn lhs(self) -> Option<Expr<'t>> {
        split_at(self.syntax, self.op()).0
    }

    pub fn rhs(self) -> Option<Expr<'t>> {
        split_at(self.syntax, self.op()).1
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    /// `-x`
    Neg,
    /// `!x`
    Not,
    /// `*x`
    Deref,
}

impl<'t> PrefixExpr<'t> {
    pub fn op(self) -> Option<SyntaxToken<'t>> {
        first_token(self.syntax)
    }

    pub fn op_kind(self) -> Option<UnaryOp> {
        let op = match self.op()?.kind() {
            T![-] => UnaryOp::Neg,
            T![!] => UnaryOp::Not,
            T![*] => UnaryOp::Deref,
            _ => return None,
        };
        Some(op)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RangeOp {
    /// `..`
    Exclusive,
    /// `..=`, or `...` in old patterns.
    Inclusive,
}

fn range_op(token: SyntaxToken<'_>) -> Option<RangeOp> {
    match token.kind() {
        T![..] => Some(RangeOp::Exclusive),
        T![..=] | T![...] => Some(RangeOp::Inclusive),
        _ => None,
    }
}

impl<'t> RangeExpr<'t> {
    pub fn op(self) -> Option<SyntaxToken<'t>> {
        first_token(self.syntax)
    }

    pub fn op_kind(self) -> Option<RangeOp> {
        range_op(self.op()?)
    }

    pub fn start(self) -> Option<Expr<'t>> {
        split_at(self.syntax, self.op()).0
    }

    pub fn end(self) -> Option<Expr<'t>> {
        split_at(self.syntax, self.op()).1
    }
}

impl<'t> RangePat<'t> {
    pub fn op(self) -> Option<SyntaxToken<'t>> {
        first_token(self.syntax)
    }

    pub fn op_kind(self) -> Option<RangeOp> {
        range_op(self.op()?)
    }

    pub fn start(self) -> Option<Pat<'t>> {
        split_at(self.syntax, self.op()).0
    }

    pub fn end(self) -> Option<Pat<'t>> {
        split_at(self.syntax, self.op()).1
    }
}

impl<'t> IndexExpr<'t> {
    pub fn base(self) -> Option<Expr<'t>> {
        support::nth_child(self.syntax, 0)
    }

    pub fn index(self) -> Option<Expr<'t>> {
        support::nth_child(self.syntax, 1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ElseBranch<'t> {
    Block(BlockExpr<'t>),
    If(IfExpr<'t>),
}

impl<'t> IfExpr<'t> {
    pub fn condition(self) -> Option<Expr<'t>> {
        support::nth_child(self.syntax, 0)
    }

    pub fn then_branch(self) -> Option<BlockExpr<'t>> {
        let block = support::nth_child::<Expr<'t>>(self.syntax, 1)?;
        BlockExpr::cast(block.syntax())
    }

    pub fn else_branch(self) -> Option<ElseBranch<'t>> {
        let else_token = support::token(self.syntax, T![else]);
        let branch = split_at::<Expr<'t>>(self.syntax, else_token).1?;
        match branch {
            Expr::BlockExpr(block) => Some(ElseBranch::Block(block)),
            Expr::IfExpr(if_expr) => Some(ElseBranch::If(if_expr)),
            _ => None,
        }
    }
}

impl<'t> WhileExpr<'t> {
    pub fn condition(self) -> Option<Expr<'t>> {
        support::nth_child(self.syntax, 0)
    }

    pub fn loop_body(self) -> Option<BlockExpr<'t>> {
        let block = support::nth_child::<Expr<'t>>(self.syntax, 1)?;
        BlockExpr::cast(block.syntax())
    }
}

impl<'t> ForExpr<'t> {
    pub fn iterable(self) -> Option<Expr<'t>> {
        support::nth_child(self.syntax, 0)
    }

    pub fn loop_body(self) -> Option<BlockExpr<'t>> {
        let block = support::nth_child::<Expr<'t>>(self.syntax, 1)?;
        BlockExpr::cast(block.syntax())
    }
}

impl<'t> Impl<'t> {
    /// `Trait` in `impl Trait for Type`.
    pub fn trait_ty(self) -> Option<Type<'t>> {
        let for_token = support::token(self.syntax, T![for])?;
        split_at(self.syntax, Some(for_token)).0
    }

    /// `Type` in `impl Type` and `impl Trait for Type`.
    pub fn self_ty(self) -> Option<Type<'t>> {
        match support::token(self.syntax, T![for]) {
            Some(for_token) => split_at(self.syntax, Some(for_token)).1,
            None => support::child(self.syntax),
        }
    }
}

impl<'t> PathSegment<'t> {
    /// `T` in a qualified path segment `<T as Trait>` or `<T>`.
    pub fn self_ty(self) -> Option<Type<'t>> {
        support::nth_child(self.syntax, 0)
    }

    /// `Trait` in a qualified path segment `<T as Trait>`.
    pub fn trait_ty(self) -> Option<PathType<'t>> {
        let ty = support::nth_child::<Type<'t>>(self.syntax, 1)?;
        PathType::cast(ty.syntax())
    }
}
