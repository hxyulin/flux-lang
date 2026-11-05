//! flux's embedded `core`, `alloc` and `std`, built once per process
//! and shared by every compilation: its files, parsed; its item tree, with names resolved;
//! its constants' values and, as they are asked for, its functions'
//! bodies, lowered.
//!
//! A program clones the frozen library tree and adds its crate after it.
//! Libraries have identical IDs in every compilation and cannot name the
//! program, so neither their files nor lazy bodies depend on a program.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

static SOURCE_LOADING_FORBIDDEN: AtomicBool = AtomicBool::new(false);

use flux_arena::ArenaMap;

use std::sync::Arc;

use crate::source::SourceFile;
use crate::{
    Body, BodySourceMap, Const, ConstId, Diagnostic, FunctionId, Hir, ItemTree, LoweredBody,
    SourceDb, StructId, lower_body,
};

static OWNED_BOX: OnceLock<Option<StructId>> = OnceLock::new();

/// `alloc`'s `Box<T>` (`#[lang = "owned_box"]`). Embedded library IDs are the
/// same in every compilation, so the ID is taken from whichever library tree
/// this process loads first: a staged built-in suite or the parsed sources.
pub fn owned_box() -> Option<StructId> {
    *OWNED_BOX.get_or_init(|| owned_box_in(&Core::get().items))
}

/// Records the library's lang structs from a loaded library tree, so that
/// later queries do not parse the library sources.
pub(crate) fn note_library(items: &ItemTree) {
    OWNED_BOX.get_or_init(|| owned_box_in(items));
}

fn owned_box_in(items: &ItemTree) -> Option<StructId> {
    items
        .lang_enums
        .get(&flux_intern::Symbol::intern("owned_box"))
        .copied()
}

/// The embedded libraries, lowered. [`Core::get`] builds them on first use.
#[derive(Debug)]
pub struct Core {
    /// Its files, and the expansions of its macro calls in item position.
    pub(crate) files: Vec<Arc<SourceFile>>,
    /// All three embedded crates, in dependency order.
    pub items: ItemTree,
    /// Function bodies, by function index, lowered when first asked for:
    /// a program uses few of them.
    bodies: Box<[OnceLock<(Body, BodySourceMap)>]>,
    /// The value of every constant that has one.
    pub(crate) const_bodies: ArenaMap<Const, (Body, BodySourceMap)>,
    /// From lowering the constants; the item tree's are in `items`.
    pub(crate) diagnostics: Vec<Diagnostic>,
    /// The embedded crates alone, as a program sees them.
    hir: OnceLock<Hir>,
}

impl Core {
    /// Permanently forbids the source library cache in this process.
    /// Drivers call this after selecting a complete checked builtin artifact.
    pub fn forbid_source_loading() {
        SOURCE_LOADING_FORBIDDEN.store(true, Ordering::Release);
    }

    pub fn get() -> &'static Core {
        assert!(
            !SOURCE_LOADING_FORBIDDEN.load(Ordering::Acquire),
            "source builtin loading is forbidden after artifact selection"
        );
        static CORE: OnceLock<Core> = OnceLock::new();
        CORE.get_or_init(Core::build)
    }

    fn build() -> Core {
        let mut db = SourceDb::core_only();
        let items = ItemTree::core(&mut db);
        note_library(&items);
        let mut diagnostics = Vec::new();
        let const_bodies = crate::lower_consts(&db, &items, 0, &mut diagnostics);
        let bodies = (0..items.functions.len())
            .map(|_| OnceLock::new())
            .collect();
        Core {
            files: db.all_files(),
            items,
            bodies,
            const_bodies,
            diagnostics,
            hir: OnceLock::new(),
        }
    }

    /// The embedded crates as a program without items of its own: a
    /// tree whose items all come before [`ItemTree::core`]'s counts, so
    /// that their bodies can be checked apart from any program.
    pub fn hir(&self) -> &Hir {
        self.hir.get_or_init(|| {
            let mut items = self.items.clone();
            items.core = items.counts();
            crate::lower_items(&SourceDb::new(), items, true)
        })
    }

    /// The body of `core`'s function `function`, if it has one, lowered
    /// now if it was not yet.
    pub fn body(&self, function: FunctionId) -> Option<&(Body, BodySourceMap)> {
        if !self.items.functions[function].has_body {
            return None;
        }
        Some(self.bodies[function.index()].get_or_init(|| {
            let lowered = self.lower_body(function);
            debug_assert!(
                lowered.diagnostics.is_empty(),
                "core lowers cleanly: {} {:?}",
                self.items.functions[function].name,
                lowered.diagnostics
            );
            (lowered.body, lowered.source_map)
        }))
    }

    /// Lowers `function` again, with its diagnostics: for tests that
    /// check `core` itself.
    pub fn lower_body(&self, function: FunctionId) -> LoweredBody {
        let file = self.items.functions[function].loc.file;
        lower_body(
            self.files[file.0 as usize].parse.tree(),
            &self.items,
            function,
        )
    }

    /// The value of constant `konst`, if it has one.
    pub fn const_body(&self, konst: ConstId) -> Option<&(Body, BodySourceMap)> {
        self.const_bodies.get(konst)
    }
}
