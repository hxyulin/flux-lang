//! A typed view of the syntax tree.
//!
//! Each node kind has a struct (`ast::Fn`, `ast::BinExpr`) wrapping a
//! [`SyntaxNode`], with accessors for its children, and each group of kinds
//! that can appear in the same place has an enum (`ast::Expr`,
//! `ast::Item`). They are `Copy` and as cheap as the node they wrap:
//! nothing is converted or allocated, accessors search the node's children.
//!
//! Accessors return `Option` or iterators because trees with syntax errors
//! can lack any part. "Typed" refers to the Rust types of the nodes, not to
//! the program's types: type checking happens later, on HIR.
//!
//! Most of this module is generated from `flux.ungram` by
//! `tests/sourcegen.rs` into `ast/generated.rs`; accessors that need to
//! tell apart children of the same type (`BinExpr::lhs` and `rhs`) are
//! written by hand in `ast/ext.rs`.

mod ext;
mod generated;

use std::marker::PhantomData;

pub use ext::{ArithOp, BinaryOp, CmpOp, ElseBranch, LiteralKind, LogicOp, RangeOp, UnaryOp};
pub use generated::*;

use crate::{Children, SyntaxElement, SyntaxKind, SyntaxNode, SyntaxToken};

/// A typed node, or an enum of them.
pub trait AstNode<'t>: Copy {
    fn can_cast(kind: SyntaxKind) -> bool;

    fn cast(syntax: SyntaxNode<'t>) -> Option<Self>;

    fn syntax(self) -> SyntaxNode<'t>;
}

/// The children of a node that cast to `N`.
#[derive(Clone)]
pub struct AstChildren<'t, N> {
    inner: Children<'t>,
    _ty: PhantomData<N>,
}

impl<'t, N: AstNode<'t>> Iterator for AstChildren<'t, N> {
    type Item = N;

    fn next(&mut self) -> Option<N> {
        self.inner.find_map(N::cast)
    }
}

mod support {
    use super::*;

    pub(super) fn child<'t, N: AstNode<'t>>(parent: SyntaxNode<'t>) -> Option<N> {
        parent.children().find_map(N::cast)
    }

    pub(super) fn children<'t, N: AstNode<'t>>(parent: SyntaxNode<'t>) -> AstChildren<'t, N> {
        AstChildren {
            inner: parent.children(),
            _ty: PhantomData,
        }
    }

    pub(super) fn nth_child<'t, N: AstNode<'t>>(parent: SyntaxNode<'t>, n: usize) -> Option<N> {
        children(parent).nth(n)
    }

    /// The first token of `kind` directly inside `parent`.
    pub(super) fn token(parent: SyntaxNode<'_>, kind: SyntaxKind) -> Option<SyntaxToken<'_>> {
        direct_tokens(parent).find(|token| token.kind() == kind)
    }

    /// The tokens directly inside `parent`, not in a child node, without
    /// trivia.
    pub(super) fn direct_tokens(parent: SyntaxNode<'_>) -> impl Iterator<Item = SyntaxToken<'_>> {
        parent
            .children_with_tokens()
            .filter_map(SyntaxElement::into_token)
            .filter(|token| !token.kind().is_trivia())
    }
}
