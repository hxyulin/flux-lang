//! The embedded libraries' MIR, built and checked once per process apart
//! from any program, for every compilation to copy.
//!
//! It is built from [`flux_ty::LibraryTypes`] in [`flux_hir::Core::hir`],
//! for the reasons given there: a library body names only library items,
//! and building and borrow-checking it asks only about them. Like their
//! inference, functions are built when a compilation first reaches them.
//! Building one needs a typed file with its inference, so the libraries'
//! typed file is lent to one builder at a time; reading what is built
//! takes no lock.

use std::sync::{Mutex, OnceLock, PoisonError};

use flux_arena::ArenaMap;
use flux_hir::{Const, ConstId, Core, FunctionId, Hir};
use flux_ty::{ClosureOwner, LibraryTypes, TypedFile};

use crate::build::Built;

/// The bodies [`crate::build_file`] takes rather than builds: the
/// embedded libraries', for the functions and constants
/// [`LibraryMir::owns`] accepts. See the module documentation.
pub(crate) struct LibraryMir {
    /// Every library constant's value, built and checked.
    consts: ArenaMap<Const, Built>,
    /// Function bodies by function index, built and checked when first
    /// asked for.
    functions: Box<[OnceLock<Built>]>,
    /// The libraries' signatures and constants, and the hidden types of
    /// their opaque types, for building; it holds a function's inference
    /// only while it is built.
    typed: Mutex<TypedFile>,
}

impl LibraryMir {
    /// The libraries' MIR, if `hir`'s item tree starts with them.
    pub(crate) fn of(hir: &Hir) -> Option<&'static LibraryMir> {
        static LIBRARY: OnceLock<LibraryMir> = OnceLock::new();
        LibraryTypes::of(hir).map(|_| LIBRARY.get_or_init(LibraryMir::build))
    }

    /// Whether `owner` is the libraries': the same items whose
    /// inference [`LibraryTypes`] gives.
    pub(crate) fn owns(&self, owner: ClosureOwner) -> bool {
        LibraryTypes::get().owns(owner)
    }

    fn build() -> LibraryMir {
        let hir = Core::get().hir();
        let types = LibraryTypes::get();
        let mut typed = TypedFile {
            sigs: types.sigs.clone(),
            bodies: ArenaMap::new(),
            consts: types.consts.clone(),
            diagnostics: Vec::new(),
        };
        // Validation reveals every opaque type a body mentions.
        for (id, _) in hir.items.functions.iter() {
            if typed.sigs.opaques.get(id).is_some_and(|o| !o.is_empty())
                && let Some(result) = types.body(id)
            {
                for (i, &ty) in result.opaques.iter().enumerate() {
                    typed.sigs.hidden.insert((id, i as u32), ty);
                }
            }
        }
        let mut consts = ArenaMap::new();
        for (id, _) in hir.const_bodies() {
            let mut built = Built::konst(hir, &typed, id);
            built.check(hir, &typed, hir.items.consts[id].loc.file, true);
            built.validate(hir, &typed, ClosureOwner::Const(id));
            consts.insert(id, built);
        }
        let functions = (0..hir.items.functions.len())
            .map(|_| OnceLock::new())
            .collect();
        LibraryMir {
            consts,
            functions,
            typed: Mutex::new(typed),
        }
    }

    /// Library constant `konst`'s value, which it must have.
    pub(crate) fn konst(&self, konst: ConstId) -> &Built {
        self.consts
            .get(konst)
            .expect("library constants built have values")
    }

    /// Library function `function`'s body, if it has one, built and
    /// checked now if it was not yet.
    pub(crate) fn function(&self, function: FunctionId) -> Option<&Built> {
        let slot = &self.functions[function.index()];
        if let Some(built) = slot.get() {
            return Some(built);
        }
        let result = LibraryTypes::get().body(function)?;
        // Every slot is filled with the lock held, so this waits for
        // whoever is building it.
        let mut typed = self.typed.lock().unwrap_or_else(PoisonError::into_inner);
        Some(slot.get_or_init(|| {
            let hir = Core::get().hir();
            typed.bodies.insert(function, result.clone());
            for (i, &ty) in result.opaques.iter().enumerate() {
                typed.sigs.hidden.insert((function, i as u32), ty);
            }
            let data = &hir.items.functions[function];
            let mut built = Built::function(hir, &typed, function);
            built.check(hir, &typed, data.loc.file, data.is_const);
            built.validate(hir, &typed, ClosureOwner::Function(function));
            typed.bodies.remove(function);
            built
        }))
    }
}
