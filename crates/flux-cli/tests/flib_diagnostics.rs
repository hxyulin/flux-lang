//! Checked artifacts preserve source diagnostics after item and file IDs shift.

use std::path::{Path, PathBuf};

use flux_cli::flib::semantic::{CheckedCrate, Dependency, load_checked, write_checked};
use flux_hir::flib::CrateIndices;
use flux_hir::{Diagnostic, Hir, SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_mir::imported::ImportedMir;
use flux_ty::flib::LoadedTypes;

const LIBRARY: &str = r#"
pub fn number() -> u32 { 1 }
fn hidden() -> u32 { 2 }
pub trait Marker {}
pub fn bounded<T: Marker>(x: T) -> T { x }
pub fn borrow<'a>(x: &'a u32) -> &'a u32 { x }
"#;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Located {
    stage: &'static str,
    path: PathBuf,
    start: u32,
    end: u32,
    message: String,
}

fn locate(hir: &Hir, stage: &'static str, diagnostic: &Diagnostic) -> Located {
    let (file, range) = hir.items.source_span(diagnostic.file, diagnostic.range);
    Located {
        stage,
        path: hir.items.files[&file].path.clone(),
        start: range.start(),
        end: range.end(),
        message: diagnostic.message.clone(),
    }
}

fn diagnostics(
    hir: &Hir,
    loaded: Option<(&LoadedTypes, &ImportedMir, &[flux_hir::FunctionId])>,
) -> Vec<Located> {
    let typed = match loaded {
        Some((types, mir, roots)) => flux_mir::check_file_from(hir, types, mir, roots),
        None => flux_mir::check_file(hir),
    };
    let mut result: Vec<_> = hir
        .diagnostics
        .iter()
        .map(|d| locate(hir, "hir", d))
        .chain(typed.diagnostics.iter().map(|d| locate(hir, "types", d)))
        .collect();
    let mut errors = hir.diagnostics.clone();
    errors.extend(typed.diagnostics.iter().cloned());
    let mir_errors = if errors.is_empty() {
        let mir = match loaded {
            Some((_, imported, roots)) => flux_mir::build_file_from(hir, &typed, imported, roots),
            None => flux_mir::build_file(hir, &typed),
        };
        assert!(mir.bugs.is_empty(), "{:?}", mir.bugs);
        mir.diagnostics
    } else {
        match loaded {
            Some((_, imported, _)) => {
                flux_mir::check_clean_bodies_from(hir, &typed, &errors, imported)
            }
            None => flux_mir::check_clean_bodies(hir, &typed, &errors),
        }
    };
    result.extend(mir_errors.iter().map(|d| locate(hir, "mir", d)));
    result.sort();
    result
}

fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        panic_strategy: flux_mir::PanicStrategy::Abort,
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    }
}

fn compare(source: &str, stage: &'static str, message: &str, span: &str) {
    let mut db = SourceDb::new();
    let a = db.add("a.rs", LIBRARY);
    let b = db.add("b.rs", source);
    let whole = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: a,
                name: Symbol::intern("a"),
                dependencies: vec![],
            },
            SourceCrate {
                root: b,
                name: Symbol::intern("b"),
                dependencies: vec![(Symbol::intern("a"), 0)],
            },
        ],
    );
    let expected = diagnostics(&whole, None);
    assert!(!expected.is_empty(), "case must produce a diagnostic");
    let start = source.find(span).unwrap() as u32;
    assert!(
        expected.iter().any(|d| d.stage == stage
            && d.path == Path::new("b.rs")
            && d.message.contains(message)
            && d.start == start
            && d.end == start + span.len() as u32),
        "missing expected {stage} diagnostic at `{span}`: {expected:#?}"
    );

    let mut db = SourceDb::new();
    let root = db.add("a.rs", LIBRARY);
    let library = flux_hir::lower(&mut db, root, Symbol::intern("a"));
    assert!(library.diagnostics.is_empty(), "{:?}", library.diagnostics);
    let typed = flux_mir::check_file(&library);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&library, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    assert!(mir.bugs.is_empty(), "{:?}", mir.bugs);
    let own = library.items.main_crate();
    let dependencies: Vec<_> = library
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != own.0 as usize)
        .map(|(index, data)| {
            let krate = flux_hir::CrateId(index as u32);
            Dependency {
                krate,
                hash: flux_hir::flib::digest(data.name.as_str().as_bytes()),
                indices: CrateIndices::collect(&library.items, krate),
            }
        })
        .collect();
    let archive = write_checked(CheckedCrate {
        items: &library.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash: [7; 32],
        dependencies: &dependencies,
        object: None,
        required_instances: &[],
        freshness: None,
    })
    .unwrap();

    let (mut db, _, offset) =
        flux_hir::lower_text("offset.rs", "pub struct Offset; fn shifted() -> u32 { 0 }");
    let mut sigs = flux_mir::check_file(&offset).sigs;
    let mut items = offset.items;
    let mut mir = empty_mir();
    let decoded = load_checked(
        &archive,
        &mut db,
        &mut items,
        &mut sigs,
        &mut mir,
        &dependencies,
    )
    .unwrap();
    assert_ne!(decoded.dependency.krate, own, "item IDs must shift");
    for &file in &decoded.dependency.indices.files {
        assert!(
            db.file(flux_hir::FileId(file))
                .parse
                .tree()
                .text()
                .is_empty()
        );
    }
    let types = LoadedTypes::new(&items, sigs, &[(decoded.dependency.krate, [7; 32])]).unwrap();
    let imported = ImportedMir::new(mir, items.counts()).unwrap();
    let roots = imported.required_functions(&items, types.signatures());
    let root = db.add("b.rs", source);
    assert_ne!(root, b, "source file IDs must shift");
    let consumer = flux_hir::lower_source_on(
        &mut db,
        items,
        root,
        Symbol::intern("b"),
        vec![(Symbol::intern("a"), decoded.dependency.krate)],
    );
    let actual = diagnostics(&consumer, Some((&types, &imported, &roots.functions)));
    assert_eq!(actual, expected);
}

#[test]
fn privacy_diagnostics_survive_metadata_loading() {
    compare(
        "fn bad() -> u32 { a::hidden() }",
        "hir",
        "private",
        "hidden",
    );
}

#[test]
fn type_diagnostics_survive_metadata_loading() {
    compare(
        "fn bad() -> bool { a::number() }",
        "types",
        "mismatched types",
        "a::number()",
    );
}

#[test]
fn trait_bound_diagnostics_survive_metadata_loading() {
    compare(
        "fn bad() { a::bounded(1u32); }",
        "types",
        "trait bound",
        "1u32",
    );
}

#[test]
fn imported_returned_reference_preserves_borrow_diagnostics() {
    compare(
        "fn bad() -> u32 { let mut x = 1u32; let r = a::borrow(&x); x = 2; *r }",
        "mir",
        "borrow",
        "x = 2",
    );
}
