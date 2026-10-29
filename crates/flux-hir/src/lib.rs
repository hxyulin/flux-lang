//! The item tree and HIR: the first semantic representation of a program.
//!
//! - The [`ItemTree`] holds every crate's modules and items with their
//!   signatures, but not their bodies, and what each module's names mean
//!   ([`def_map`]). Everything outside a body depends only on it. A
//!   compilation has embedded `core`, `alloc` and the bounded `std`
//!   facade, lowered once per process ([`Core`]), then the program.
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
mod cfg;
pub mod def_map;
pub mod flib;
mod inputs;
mod intrinsic;
pub use inputs::TrackedInputs;
pub use intrinsic::Intrinsic;
mod item_tree;
mod library;
mod literal;
mod lower;
mod macros;
mod pretty;
mod proc_macro;
pub use proc_macro::{
    ProcMacro, ProcMacroExpander, ProcMacroInput, ProcMacroKind, ProcMacroResolver, ProcMacroToken,
    proc_macro_declarations,
};
mod source;

pub use body::{
    AsmOptions, AsmRegister, AsmTemplatePiece, Binding, BindingId, Body, BodyLists, BodySourceMap,
    Expr, ExprId, InlineAsm, Literal, LoopSource, MatchArm, Pat, PatId, PathGenerics, Res,
    SegmentArgs, Stmt, TypeRefId,
};
pub use flux_syntax::ast::{ArithOp, BinaryOp, CmpOp, LogicOp, UnaryOp};
pub use item_tree::{
    ArrayLen, ArrayLenKind, AssocBinding, AssocItem, Bound, Const, ConstId, CrateData, CrateId,
    Derive, Discriminant, DynRef, Field, Function, FunctionId, GenericArgs, GenericParam, Generics,
    Impl, ImplId, ImplTarget, ImplTraitRef, ImplType, ItemCounts, ItemLoc, ItemTree, LifetimeParam,
    LifetimeRef, MISPLACED_BINDING, Module, ModuleId, OutlivesPredicate, Owner, Param, ParamKind,
    Path, PathAnchor, QSelf, SizedDispatch, Struct, StructId, StructKind, Trait, TraitId,
    TraitType, TypeAlias, TypeAliasId, TypeOutlives, TypeRef, UseCapture, Variant, VariantData,
    Visibility, WherePredicate, array_target, is_primitive, raw_ptr_target, slice_target,
};
pub use library::Core;
pub use lower::{LoweredBody, lower_body, lower_const};
pub use macros::{Builtin, MacroDef, MacroId, MacroKind, MacroScope, MacroScopeId};
pub use source::{CORE_ROOT, CrateCfg, FileId, SourceDb, SourceFile};

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
/// and constant of the program that has one. `core`'s come from [`Core`].
#[derive(Debug)]
pub struct Hir {
    pub items: ItemTree,
    /// The program's function bodies.
    pub bodies: ArenaMap<Function, (Body, BodySourceMap)>,
    /// The value of every constant of the program that has one.
    const_bodies: ArenaMap<Const, (Body, BodySourceMap)>,
    /// From the item tree and every body, by file and position.
    pub diagnostics: Vec<Diagnostic>,
}

/// Builds the item tree of the crate whose root file is `root`, after
/// `core`'s, and lowers every body.
pub fn lower(db: &mut SourceDb, root: FileId, crate_name: Symbol) -> Hir {
    lower_with(db, root, crate_name, true)
}

fn lower_with(db: &mut SourceDb, root: FileId, crate_name: Symbol, with_core: bool) -> Hir {
    let items = ItemTree::new(db, root, crate_name, with_core);
    lower_items(db, items, with_core)
}

/// A source crate and direct edges to earlier entries in a compilation graph.
#[derive(Debug, Clone)]
pub struct SourceCrate {
    pub root: FileId,
    pub name: Symbol,
    pub dependencies: Vec<(Symbol, usize)>,
}

/// Lowers a nonempty source graph; the final crate is the executable.
pub fn lower_sources(db: &mut SourceDb, sources: &[SourceCrate]) -> Hir {
    assert!(!sources.is_empty(), "expected an executable source crate");
    let items = ItemTree::from_sources(db, sources);
    lower_items(db, items, true)
}

/// Appends a source graph to an existing checked prefix without loading source libraries.
pub fn lower_sources_on(db: &mut SourceDb, mut items: ItemTree, sources: &[SourceCrate]) -> Hir {
    assert!(!sources.is_empty(), "expected an executable source crate");
    let from = items.counts();
    let base = items.crates.len() as u32;
    for (index, source) in sources.iter().enumerate() {
        let dependencies = source
            .dependencies
            .iter()
            .map(|&(name, target)| {
                assert!(
                    target < index,
                    "source dependencies must precede their consumer"
                );
                (name, CrateId(base + target as u32))
            })
            .collect();
        items.add_source_crate(db, source.root, source.name, false, dependencies);
    }
    lower_items_from(db, items, true, from)
}

/// Collects and lowers one source crate after previously loaded artifact crates.
/// Dependency IDs refer to existing crates in `items`.
pub fn lower_source_on(
    db: &mut SourceDb,
    mut items: ItemTree,
    root: FileId,
    name: Symbol,
    dependencies: Vec<(Symbol, CrateId)>,
) -> Hir {
    let from = items.counts();
    items.add_source_crate(db, root, name, false, dependencies);
    lower_items_from(db, items, true, from)
}

fn lower_items(db: &SourceDb, items: ItemTree, with_core: bool) -> Hir {
    let from = items.core;
    lower_items_from(db, items, with_core, from)
}

fn lower_items_from(
    db: &SourceDb,
    mut items: ItemTree,
    with_core: bool,
    from: item_tree::ItemCounts,
) -> Hir {
    let mut diagnostics = std::mem::take(&mut items.diagnostics);
    if with_core && !items.has_artifact_builtins() {
        diagnostics.extend(Core::get().diagnostics.iter().cloned());
    }
    let mut bodies = ArenaMap::new();
    let functions = items.functions.iter().skip(from.functions as usize);
    for (id, function) in functions {
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
    let const_bodies = lower_consts(db, &items, from.consts as usize, &mut diagnostics);
    for d in &mut diagnostics {
        items.resolve_diagnostic(d);
    }
    Diagnostic::sort(&mut diagnostics);
    Hir {
        items,
        bodies,
        const_bodies,
        diagnostics,
    }
}

/// Lowers the value of every constant from `from` on that has one.
fn lower_consts(
    db: &SourceDb,
    items: &ItemTree,
    from: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> ArenaMap<Const, (Body, BodySourceMap)> {
    let mut const_bodies = ArenaMap::new();
    for (id, konst) in items.consts.iter().skip(from) {
        if konst.has_body {
            let tree = db.file(konst.loc.file).parse.tree();
            let lowered = lower_const(tree, items, id);
            diagnostics.extend(lowered.diagnostics.into_iter().map(|mut d| {
                d.file = konst.loc.file;
                d
            }));
            const_bodies.insert(id, (lowered.body, lowered.source_map));
        }
    }
    const_bodies
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

    /// The body of `function`, if it has one: the program's, or `core`'s,
    /// lowered now if it was not yet.
    pub fn body(&self, function: FunctionId) -> Option<&(Body, BodySourceMap)> {
        if let Some(body) = self.bodies.get(function) {
            return Some(body);
        }
        if self.is_program(function) || self.items.has_artifact_builtins() {
            return None;
        }
        Core::get().body(function)
    }

    /// Lowers `core`'s function `function` again, with its diagnostics:
    /// for tests that check `core` itself.
    pub fn lower_core_body(&self, function: FunctionId) -> LoweredBody {
        assert!(
            !self.items.has_artifact_builtins(),
            "artifact builtins have no HIR source bodies"
        );
        Core::get().lower_body(function)
    }

    /// The value of constant `konst`, if it has one.
    pub fn const_body(&self, konst: ConstId) -> Option<&(Body, BodySourceMap)> {
        if konst.index() < self.items.core.consts as usize {
            if self.items.has_artifact_builtins() {
                return None;
            }
            return Core::get().const_body(konst);
        }
        self.const_bodies.get(konst)
    }

    /// Every constant's value, `core`'s first.
    pub fn const_bodies(&self) -> impl Iterator<Item = (ConstId, &(Body, BodySourceMap))> {
        let core = (self.items.core.consts > 0 && !self.items.has_artifact_builtins())
            .then(|| Core::get().const_bodies.iter());
        core.into_iter().flatten().chain(self.const_bodies.iter())
    }

    /// Whether `function` belongs to an input source crate, rather than
    /// the cached embedded libraries. This includes source dependencies.
    pub fn is_program(&self, function: FunctionId) -> bool {
        function.index() >= self.items.core.functions as usize
    }
}
