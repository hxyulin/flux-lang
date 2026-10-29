//! Native metadata round trip independent of the driver's pending cache integration.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod flib_support;

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
    let source_dir =
        std::env::temp_dir().join(format!("flux-flib-native-sources-{}", std::process::id()));
    std::fs::create_dir(&source_dir).unwrap();
    struct RemoveSources(std::path::PathBuf);
    impl Drop for RemoveSources {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _sources = RemoveSources(source_dir.clone());
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
        let root=db.add("downstream.rs","fn main() { assert!(a::id(a::double(21)) == 42); assert!(a::location() == 1); assert!(a::generic_location(0u8) == 3); }");
        let hir = flux_hir::lower_source_on(
            &mut db,
            items,
            root,
            Symbol::intern("downstream"),
            vec![(Symbol::intern("a"), a_id)],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let (sigs, errors) = flux_ty::signatures_from(&hir.items, base_types.sigs, counts, None);
        assert!(errors.is_empty(), "{errors:?}");
        let mut typed = flux_ty::TypedFile {
            sigs,
            bodies: base_types.bodies,
            consts: base_types.consts,
            diagnostics: Vec::new(),
        };
        let provider = flux_mir::imported::ImportedMir::new(imported_mir, counts).unwrap();
        let required_functions = provider.required_functions(&hir.items, &typed.sigs);
        let mut queue = required_functions.clone();
        queue.extend(hir.bodies.iter().map(|(id, _)| id));
        while let Some(id) = queue.pop() {
            if typed.bodies.get(id).is_some() {
                continue;
            }
            let Some((body, map)) = hir.body(id) else {
                continue;
            };
            let result = flux_ty::infer_body(&hir.items, &typed.sigs, id, body, map);
            assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
            for (index, &ty) in result.opaques.iter().enumerate() {
                typed.sigs.hidden.insert((id, index as u32), ty);
            }
            typed.bodies.insert(id, result);
            let body = flux_mir::build(&hir, &typed, id);
            for called in body
                .callees()
                .into_iter()
                .chain(body.object_methods(&hir.items))
            {
                queue.extend(flux_ty::call_targets(&hir.items, called));
            }
        }
        let mut mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        // The production cache seam will supply these compiler-owned checked
        // templates. They are deliberately absent from a user artifact.
        for (id, body) in a_mir.bodies.iter() {
            if id.index() < hir.items.core.functions as usize && mir.bodies.get(id).is_none() {
                mir.bodies.insert(id, body.clone());
            }
        }
        for (&key, body) in &a_mir.closures {
            let embedded = match key.0.owner {
                flux_ty::ClosureOwner::Function(f) => f.index() < hir.items.core.functions as usize,
                flux_ty::ClosureOwner::Const(c) => c.index() < hir.items.core.consts as usize,
            };
            if embedded {
                mir.closures.entry(key).or_insert_with(|| body.clone());
            }
        }
        provider.seed_into(&mut mir).unwrap();
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
        let directory =
            std::env::temp_dir().join(format!("flux-flib-native-{}-{tier:?}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        struct Remove(std::path::PathBuf);
        impl Drop for Remove {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _remove = Remove(directory.clone());
        let output = directory.join("program");
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
