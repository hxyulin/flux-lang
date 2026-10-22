//! flux's own `core`, built once per process and shared by every
//! compilation: its files, parsed; its item tree, with names resolved;
//! its constants' values and, as they are asked for, its functions'
//! bodies, lowered.
//!
//! A program's item tree starts as a copy of `core`'s and adds the
//! program's crate after it, so `core`'s items have the same ids in every
//! compilation. `core` cannot name the program, so nothing here depends
//! on one.

use std::sync::OnceLock;

use flux_arena::ArenaMap;

use std::sync::Arc;

use crate::source::SourceFile;
use crate::{
    Body, BodySourceMap, Const, ConstId, Diagnostic, FunctionId, ItemTree, LoweredBody, SourceDb,
    lower_body,
};

/// `core`, lowered. [`Core::get`] builds it the first time it is asked
/// for.
#[derive(Debug)]
pub struct Core {
    /// Its files, and the expansions of its macro calls in item position.
    pub(crate) files: Vec<Arc<SourceFile>>,
    /// `core`'s crate alone.
    pub items: ItemTree,
    /// Function bodies, by function index, lowered when first asked for:
    /// a program uses few of them.
    bodies: Box<[OnceLock<(Body, BodySourceMap)>]>,
    /// The value of every constant that has one.
    pub(crate) const_bodies: ArenaMap<Const, (Body, BodySourceMap)>,
    /// From lowering the constants; the item tree's are in `items`.
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl Core {
    pub fn get() -> &'static Core {
        static CORE: OnceLock<Core> = OnceLock::new();
        CORE.get_or_init(Core::build)
    }

    fn build() -> Core {
        let mut db = SourceDb::core_only();
        let items = ItemTree::core(&mut db);
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
        }
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
