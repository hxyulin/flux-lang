//! The embedded libraries' bodies, inferred once per process apart from
//! any program, for every compilation to copy.
//!
//! A library body names only library items, which come before a
//! program's in its item tree and keep their ids, so the library's
//! signatures and [`flux_hir::Core::hir`] are all its inference needs.
//! A program's `impl` blocks are left out, as rustc type-checks `core`
//! before the crates that use it: they cannot make a library body's
//! selection ambiguous. Nothing else of a compilation reaches these
//! bodies: `cfg` and the target are the host's, fixed in the process,
//! and `-O` changes only code generation.
//!
//! A body whose array lengths name constants that are not literals, or
//! that finds the types of const arguments, is inferred again with the
//! program's evaluator, so its result cannot be shared. The libraries
//! have no such body; inferring each one asserts it.

use std::sync::OnceLock;

use flux_arena::ArenaMap;
use flux_hir::{Const, ConstId, Core, FunctionId, Hir, ItemCounts};

use crate::{
    ClosureOwner, InferenceResult, Signatures, infer_body, infer_const, lens_signatures,
    literal_lens,
};

/// The results type checking takes rather than infers: the embedded
/// libraries' inference, built by [`LibraryTypes::get`].
///
/// [`check_file`](crate::check_file) asks it, through [`LibraryTypes::owns`]
/// alone, for every body and constant it owns. That relies on the
/// libraries' items being a prefix of every program's item tree, with
/// the same ids, and on their bodies naming nothing after it.
pub struct LibraryTypes {
    /// The libraries' item counts: the ids below them are theirs.
    counts: ItemCounts,
    /// The libraries' signatures, as a program without items has them.
    pub sigs: Signatures,
    /// Every library constant's value, inferred.
    pub consts: ArenaMap<Const, InferenceResult>,
    /// Function bodies by function index, inferred when first asked for.
    bodies: Box<[OnceLock<InferenceResult>]>,
}

impl LibraryTypes {
    /// The libraries' inference, built the first time it is asked for.
    pub fn get() -> &'static LibraryTypes {
        static LIBRARY: OnceLock<LibraryTypes> = OnceLock::new();
        LIBRARY.get_or_init(LibraryTypes::build)
    }

    /// The libraries' inference, if `hir`'s item tree starts with them.
    pub fn of(hir: &Hir) -> Option<&'static LibraryTypes> {
        (hir.items.core.functions > 0 && !hir.items.has_artifact_builtins()).then(LibraryTypes::get)
    }

    /// Whether `owner` is one of the libraries' functions or constants,
    /// whose results [`LibraryTypes::body`] and [`LibraryTypes::konst`]
    /// give.
    pub fn owns(&self, owner: ClosureOwner) -> bool {
        match owner {
            ClosureOwner::Function(f) => f.index() < self.counts.functions as usize,
            ClosureOwner::Const(c) => c.index() < self.counts.consts as usize,
        }
    }

    fn build() -> LibraryTypes {
        let hir = Core::get().hir();
        // The libraries' signature diagnostics are every program's too.
        let (sigs, _) = lens_signatures(hir, false, &literal_lens);
        let mut consts = ArenaMap::new();
        for (id, (body, source_map)) in hir.const_bodies() {
            let result = infer_const(&hir.items, &sigs, id, body, source_map);
            consts.insert(id, shared(result, || hir.items.const_name(id).to_string()));
        }
        let bodies = (0..hir.items.functions.len())
            .map(|_| OnceLock::new())
            .collect();
        LibraryTypes {
            counts: hir.items.core,
            sigs,
            consts,
            bodies,
        }
    }

    /// Library function `function`'s body inferred, if it has one,
    /// inferred now if it was not yet.
    pub fn body(&self, function: FunctionId) -> Option<&InferenceResult> {
        let hir = Core::get().hir();
        let (body, source_map) = hir.body(function)?;
        Some(self.bodies[function.index()].get_or_init(|| {
            let result = infer_body(&hir.items, &self.sigs, function, body, source_map);
            shared(result, || {
                hir.items.function_path(&hir.items.functions[function])
            })
        }))
    }

    /// Library constant `konst`'s value inferred; it must have one.
    pub fn konst(&self, konst: ConstId) -> &InferenceResult {
        self.consts
            .get(konst)
            .expect("library constants with values are inferred")
    }
}

/// `result`, which every program can share: one that waits for
/// evaluation would be inferred again with the program's evaluator.
fn shared(result: InferenceResult, name: impl FnOnce() -> String) -> InferenceResult {
    assert!(
        result.missing_lens.is_empty() && result.new_arg_tys.is_empty(),
        "library body `{}` needs constants evaluated before inference",
        name()
    );
    result
}
