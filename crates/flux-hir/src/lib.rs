//! The item tree and HIR: the first semantic representation of a program.
//!
//! - The [`ItemTree`] holds every crate's modules and items with their
//!   signatures, but not their bodies, and what each module's names mean
//!   ([`def_map`]). Everything outside a body depends only on it. A
//!   compilation has two crates: flux's own `core`, built into the
//!   compiler, and the program.
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
pub mod def_map;
mod item_tree;
mod literal;
mod lower;
mod pretty;
mod source;

pub use body::{
    Binding, BindingId, Body, BodySourceMap, Expr, ExprId, Literal, LoopSource, Pat, PatId,
    PathGenerics, Res, SegmentArgs, Stmt, TypeRefId,
};
pub use flux_syntax::ast::{ArithOp, BinaryOp, CmpOp, LogicOp, UnaryOp};
pub use item_tree::{
    AssocItem, Const, ConstId, CrateData, CrateId, Field, Function, FunctionId, GenericArgs,
    GenericParam, Generics, Impl, ImplId, ImplTarget, ItemLoc, ItemTree, Module, ModuleId, Param,
    Path, PathAnchor, Struct, StructId, StructKind, TypeRef, Visibility, is_primitive,
};
pub use lower::{lower_body, lower_const};
pub use source::{CORE_ROOT, FileId, SourceDb, SourceFile};

use flux_arena::ArenaMap;
use flux_intern::Symbol;
use flux_syntax::TextRange;

/// An error found while building the item tree or lowering a body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub file: FileId,
    pub range: TextRange,
    pub message: String,
}

impl Diagnostic {
    /// A diagnostic whose file is given later, by whoever knows it.
    pub fn new(range: TextRange, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            file: FileId::UNKNOWN,
            range,
            message: message.into(),
        }
    }

    /// Sorts diagnostics by file and position.
    pub fn sort(diagnostics: &mut [Diagnostic]) {
        diagnostics.sort_by_key(|d| (d.file, d.range.start(), d.range.end()));
    }
}

/// A whole program lowered: its item tree and the body of every function
/// that has one.
#[derive(Debug)]
pub struct Hir {
    pub items: ItemTree,
    pub bodies: ArenaMap<Function, (Body, BodySourceMap)>,
    /// The value of every constant that has one.
    pub const_bodies: ArenaMap<Const, (Body, BodySourceMap)>,
    /// From the item tree and every body, by file and position.
    pub diagnostics: Vec<Diagnostic>,
}

/// Builds the item tree of `core` and of the crate whose root file is
/// `root`, and lowers every body.
pub fn lower(db: &mut SourceDb, root: FileId, crate_name: Symbol) -> Hir {
    lower_with(db, root, crate_name, true)
}

fn lower_with(db: &mut SourceDb, root: FileId, crate_name: Symbol, with_core: bool) -> Hir {
    let items = ItemTree::new(db, root, crate_name, with_core);
    let mut diagnostics = items.diagnostics.clone();
    let mut bodies = ArenaMap::new();
    for (id, function) in items.functions.iter() {
        if function.has_body {
            let tree = db.file(function.loc.file).parse.tree();
            let lowered = lower_body(tree, &items, id);
            diagnostics.extend(lowered.diagnostics.into_iter().map(|mut d| {
                d.file = function.loc.file;
                d
            }));
            bodies.insert(id, (lowered.body, lowered.source_map));
        }
    }
    let mut const_bodies = ArenaMap::new();
    for (id, konst) in items.consts.iter() {
        if konst.has_body {
            let tree = db.file(konst.loc.file).parse.tree();
            let lowered = lower_const(tree, &items, id);
            diagnostics.extend(lowered.diagnostics.into_iter().map(|mut d| {
                d.file = konst.loc.file;
                d
            }));
            const_bodies.insert(id, (lowered.body, lowered.source_map));
        }
    }
    Diagnostic::sort(&mut diagnostics);
    Hir {
        items,
        bodies,
        const_bodies,
        diagnostics,
    }
}

/// Lowers `text` as the root file, at `path`, of a program called `main`;
/// for tests and tools that have one file in hand.
pub fn lower_text(path: impl Into<std::path::PathBuf>, text: &str) -> (SourceDb, FileId, Hir) {
    let mut db = SourceDb::new();
    let root = db.add(path, text);
    let hir = lower(&mut db, root, Symbol::intern("main"));
    (db, root, hir)
}

/// Like [`lower_text`], without `core`: for tests that lower many
/// programs and care only about their own code.
pub fn lower_text_bare(path: impl Into<std::path::PathBuf>, text: &str) -> (SourceDb, FileId, Hir) {
    let mut db = SourceDb::new();
    let root = db.add(path, text);
    let hir = lower_with(&mut db, root, Symbol::intern("main"), false);
    (db, root, hir)
}

impl Hir {
    /// The program's items and bodies, `core`'s left out, in a readable,
    /// Rust-like form, with each local written `name@index`. Used by
    /// snapshot tests and `flux hir`.
    pub fn debug_dump(&self) -> String {
        pretty::file(self)
    }

    /// `diagnostics` as debug dumps write them: `error 12..34: message`,
    /// with a file number for files other than the program's root.
    pub fn write_diagnostics(&self, out: &mut String, diagnostics: &[Diagnostic]) {
        pretty::write_diagnostics(out, self, diagnostics);
    }

    /// Whether `function` is the program's, rather than `core`'s.
    pub fn is_program(&self, function: FunctionId) -> bool {
        self.items
            .in_main_crate(self.items.functions[function].loc.module)
    }
}
