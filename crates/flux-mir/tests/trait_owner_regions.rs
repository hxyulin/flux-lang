//! Trait method owner contracts constrain calls independently of the declaration gate.
use std::process::Command;

#[test]
fn trait_method_owner_obligations_match_rustc() {
    check_cases("trait_owner_regions");
}

// These retain the desired assertions while the separately owned proofs land.
#[test]
#[ignore = "requires generic caller trait-region proof integration"]
fn abstract_trait_owner_integration_targets() {
    check_cases("trait_owner_generic_integration");
}

#[test]
#[ignore = "requires owner-bound FnDef HRTB boundary integration"]
fn higher_ranked_owner_integration_targets() {
    check_cases("trait_owner_hrtb_integration");
}

fn check_cases(directory: &str) {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data")
        .join(directory);
    let dir = std::env::temp_dir().join(format!("flux-{directory}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for entry in std::fs::read_dir(fixtures).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_stem().unwrap().to_str().unwrap();
        let accepts = name.ends_with("_good");
        let source = std::fs::read_to_string(&path).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let gate_count = hir
            .items
            .traits
            .values()
            .map(|t| t.generics.lifetimes.len())
            .sum::<usize>();
        assert_eq!(
            typed.diagnostics.len(),
            gate_count,
            "{name}: {:?}",
            typed.diagnostics
        );
        assert!(
            typed
                .diagnostics
                .iter()
                .all(|d| d.message == "lifetime parameters on traits are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
        // Preserve every original type diagnostic; only inspect the independent
        // MIR result below. This is never public acceptance of a gated trait.
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepts,
            "{name}: {:?}",
            mir.diagnostics
        );
        if !accepts {
            assert!(
                mir.diagnostics
                    .iter()
                    .any(|d| d.message.contains("lifetime")
                        || d.message.contains("escapes")
                        || d.message.contains("does not live long enough")),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn malformed_trait_owner_metadata_fails_closed() {
    let source = "trait Tr<'a> {fn take(x:&'a i32);} struct S; impl<'a> Tr<'a> for S {fn take(_:&'a i32){}} fn f<'a,N:Tr<'a>>(x:&'a i32){N::take(x);} fn main(){}";
    for callback in [false, true] {
        let source = if callback {
            source.replace("N::take(x);", "[x].into_iter().for_each(N::take);")
        } else {
            source.to_owned()
        };
        let (_, _, hir) = flux_hir::lower_text("owner_metadata.rs", &source);
        let (method, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| {
                f.name.as_str() == "take"
                    && matches!(f.owner, Some(flux_hir::Owner::Trait(t))
                        if hir.items.traits[t].name.as_str() == "Tr")
            })
            .unwrap();
        let flux_hir::Owner::Trait(owner) = hir.items.functions[method].owner.unwrap() else {
            unreachable!()
        };
        for corruption in 0..7 {
            let mut typed = flux_mir::check_file(&hir);
            assert_eq!(typed.diagnostics.len(), 1);
            match corruption {
                0 => typed.sigs.regions.fns.get_mut(method).unwrap().owner = 0,
                1 => typed.sigs.regions.fns.get_mut(method).unwrap().owner = 2,
                2 => typed.sigs.regions.fns.get_mut(method).unwrap().names[0].name = None,
                3 => typed.sigs.regions.traits.get_mut(owner).unwrap().names = Box::default(),
                4 => typed.sigs.regions.traits = Default::default(),
                5 => {
                    typed.sigs.regions.fns.get_mut(method).unwrap().owner = 0;
                    typed.sigs.regions.traits.get_mut(owner).unwrap().names = Box::default();
                }
                _ => {
                    let old = std::mem::take(&mut typed.sigs.regions.fns);
                    for (id, regions) in old.iter().filter(|(id, _)| *id != method) {
                        typed.sigs.regions.fns.insert(id, regions.clone());
                    }
                }
            }
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(
                mir.diagnostics.iter().any(|d| {
                    d.message == "trait method owner region metadata is not supported yet"
                }),
                "corruption {corruption}: {:?}",
                mir.diagnostics
            );
        }
    }
}
