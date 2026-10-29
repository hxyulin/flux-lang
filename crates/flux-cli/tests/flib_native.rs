//! Native metadata round trip through the checked artifact providers.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod flib_support;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

use flux_cli::flib::semantic::{CheckedCrate, Dependency, load_checked, write_checked};
use flux_hir::flib::{CrateIndices, digest};
use flux_intern::Symbol;

fn empty_mir() -> flux_mir::FileMir {
    flux_mir::FileMir {
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    }
}

#[test]
fn decoded_generic_mir_and_library_object_link_into_downstream_program() {
    let sources = scratch::Scratch::new("out/flib-native/sources");
    let source_dir = sources.path();
    let included = source_dir.join("api.rs");
    std::fs::write(&included,"pub fn double(x:u32)->u32 { x * 2 } pub fn id<T>(x:T)->T { x }\n#[track_caller] pub fn location()->u32 { std::panic::Location::caller().line() }\npub fn generic_location<T>(_:T)->u32 { location() }\n").unwrap();
    let (_db, _, a) = flux_hir::lower_text(
        source_dir.join("removed-dependency.rs"),
        "include!(\"api.rs\");",
    );
    std::fs::remove_file(&included).unwrap();
    let a_types = flux_mir::check_file(&a);
    assert!(a_types.diagnostics.is_empty(), "{:?}", a_types.diagnostics);
    let a_mir = flux_mir::build_file(&a, &a_types);
    assert!(a_mir.diagnostics.is_empty(), "{:?}", a_mir.diagnostics);
    let own = a.items.main_crate();
    let dependencies: Vec<_> = a
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != own.0 as usize)
        .map(|(i, c)| {
            let krate = flux_hir::CrateId(i as u32);
            Dependency {
                krate,
                hash: digest(c.name.as_str().as_bytes()),
                indices: CrateIndices::collect(&a.items, krate),
            }
        })
        .collect();
    let a_hash = [77; 32];
    let a_hashes: Vec<_> = dependencies
        .iter()
        .map(|d| (d.krate, d.hash))
        .chain(std::iter::once((own, a_hash)))
        .collect();
    let target = flux_codegen::Target::Aarch64AppleDarwin;
    for tier in [
        flux_codegen::OptimizationTier::None,
        flux_codegen::OptimizationTier::Full,
    ] {
        let mut module = flux_codegen::lower_crate(
            &a,
            &a_types,
            &a_mir,
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::Library(own),
                crate_hashes: &a_hashes,
                imported: &[],
                required_functions: &[],
            },
        )
        .unwrap();
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let artifact = write_checked(CheckedCrate {
            items: &a.items,
            sigs: &a_types.sigs,
            mir: &a_mir,
            krate: own,
            hash: a_hash,
            dependencies: &dependencies,
            object: Some(object),
            freshness: None,
        })
        .unwrap();
        // Insert unrelated items first so all user-crate IDs move. A's source
        // exists only as diagnostic text in the artifact, never as a source input.
        std::fs::write(
            source_dir.join("padding.rs"),
            "fn unrelated() { let pad = 12345; }",
        )
        .unwrap();
        let (mut db, _, base) =
            flux_hir::lower_text(source_dir.join("unrelated.rs"), "include!(\"padding.rs\");");
        let mut base_types = flux_mir::check_file(&base);
        let mut items = base.items;
        let mut imported_mir = empty_mir();
        let loaded = load_checked(
            &artifact,
            &mut db,
            &mut items,
            &mut base_types.sigs,
            &mut imported_mir,
            &dependencies,
        )
        .unwrap();
        let counts = items.counts();
        let a_id = loaded.dependency.krate;
        let loaded_types =
            flux_ty::flib::LoadedTypes::new(&items, base_types.sigs, &[(a_id, a_hash)]).unwrap();
        let provider = flux_mir::imported::ImportedMir::new(imported_mir, counts).unwrap();
        let root=db.add("downstream.rs","fn main() { assert!(a::id(a::double(21)) == 42); assert!(a::location() == 1); assert!(a::generic_location(0u8) == 3); }");
        let hir = flux_hir::lower_source_on(
            &mut db,
            items,
            root,
            Symbol::intern("downstream"),
            vec![(Symbol::intern("a"), a_id)],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let required_functions = provider.required_functions(&hir.items, loaded_types.signatures());
        let typed = flux_mir::check_file_from(&hir, &loaded_types, &provider, &required_functions);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file_from(&hir, &typed, &provider, &required_functions);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        assert!(mir.bugs.is_empty(), "{:?}", mir.bugs);
        let b_id = hir.items.main_crate();
        let hashes: Vec<_> = dependencies
            .iter()
            .map(|d| (d.krate, d.hash))
            .chain([(a_id, a_hash), (b_id, [88; 32])])
            .collect();
        let mut module = flux_codegen::lower_crate(
            &hir,
            &typed,
            &mir,
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::Executable,
                crate_hashes: &hashes,
                imported: &[a_id],
                required_functions: &required_functions,
            },
        )
        .unwrap();
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let directory = scratch::Scratch::new("out/flib-native/programs");
        let output = directory.path().join("program");
        flux_codegen::link_objects(
            &[
                flux_codegen::ObjectInput {
                    name: "dependency",
                    bytes: loaded.object.as_ref().unwrap(),
                },
                flux_codegen::ObjectInput {
                    name: "downstream",
                    bytes: &object,
                },
            ],
            target,
            &output,
            &[],
        )
        .unwrap();
        let result = flib_support::run(&mut std::process::Command::new(output), "native");
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
