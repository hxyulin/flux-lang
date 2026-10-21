//! The item tree and HIR: the first semantic representation of a file.
//!
//! - The [`ItemTree`] holds a file's items and their signatures, but not
//!   their bodies. Everything outside a body depends only on it.
//! - A [`Body`] is one function's code, lowered from the typed AST into
//!   arenas of expressions, patterns and bindings. Names are resolved
//!   during lowering ([`Res`]), `while` becomes `loop`, and parentheses
//!   disappear. A [`BodySourceMap`] maps each node back to its source
//!   range for diagnostics.
//!
//! Bodies are lowered independently of each other given the item tree, so
//! they can be lowered (and later type-checked) in parallel, and an edit
//! inside one body leaves every other body unchanged.
//!
//! Syntax outside the supported subset lowers to [`Expr::Missing`] or
//! [`Pat::Missing`] with a "not supported yet" diagnostic, so nothing is
//! silently ignored.

mod body;
mod item_tree;
mod literal;
mod lower;
mod pretty;

pub use body::{
    Binding, BindingId, Body, BodySourceMap, Expr, ExprId, Literal, LoopSource, Pat, PatId, Res,
    Stmt, TypeRefId,
};
pub use flux_syntax::ast::{ArithOp, BinaryOp, CmpOp, LogicOp, UnaryOp};
pub use item_tree::{Function, FunctionId, ItemTree, Param, TypeRef};
pub use lower::lower_body;

use flux_arena::ArenaMap;
use flux_syntax::{Parse, TextRange};

/// An error found while building the item tree or lowering a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub range: TextRange,
    pub message: String,
}

impl Diagnostic {
    pub(crate) fn new(range: TextRange, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            range,
            message: message.into(),
        }
    }
}

/// A whole file lowered: its item tree and the body of every function
/// that has one.
#[derive(Debug)]
pub struct FileHir {
    pub items: ItemTree,
    pub bodies: ArenaMap<Function, (Body, BodySourceMap)>,
    /// From the item tree and every body, in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// Builds the item tree of `parse` and lowers every body.
pub fn lower_file(parse: &Parse) -> FileHir {
    let items = ItemTree::new(parse.tree());
    let mut diagnostics = items.diagnostics.clone();
    let mut bodies = ArenaMap::new();
    for (id, function) in items.functions.iter() {
        if function.has_body {
            let lowered = lower_body(parse.tree(), &items, id);
            diagnostics.extend(lowered.diagnostics);
            bodies.insert(id, (lowered.body, lowered.source_map));
        }
    }
    diagnostics.sort_by_key(|d| (d.range.start(), d.range.end()));
    FileHir {
        items,
        bodies,
        diagnostics,
    }
}

impl FileHir {
    /// The item tree and every body in a readable, Rust-like form, with
    /// each local written `name@index`. Used by snapshot tests and
    /// `flux hir`.
    pub fn debug_dump(&self) -> String {
        pretty::file(self)
    }
}
