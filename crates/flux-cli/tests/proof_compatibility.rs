//! Independent Rustc metadata and Flux semantic/MIR oracles. Invalid Rust is
//! compile-only; native matching controls live in tests/run.
use std::path::Path;
use std::process::Command;

#[derive(serde::Deserialize)]
struct Case {
    name: String,
    source: String,
    accepts: bool,
}

#[test]
fn caller_proof_compatibility_matches_rustc() {
    // Includes all 99 independent review probes, the exact reductions and
    // matching controls, both direct-bound orders, transitive/diamond proofs,
    // distinct/permuted arguments, multiple names and generic right sides.
    check_cases(
        include_str!("fixtures/proof_equalities/compatibility.json"),
        "oracles",
        144,
    );
}

#[test]
fn trait_header_proof_provenance_matches_rustc() {
    // Header where-clauses and `type Part: P<A = ...>` declarations share
    // a trait's old projection table, but must retain different provenance.
    check_cases(
        include_str!("fixtures/proof_equalities/headers.json"),
        "headers",
        4,
    );
}

fn check_cases(source: &str, group: &str, count: usize) {
    let cases: Vec<Case> = serde_json::from_str(source).unwrap();
    assert_eq!(cases.len(), count);
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!("out/proof-compat/{group}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for case in cases {
        let name = &case.name;
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, &case.source).unwrap();
        let rustc = Command::new("rustc")
            .args([
                "--edition=2024",
                "--emit=metadata",
                "--crate-name=proof_compatibility",
                "-Awarnings",
            ])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            case.accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&file, &case.source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            case.accepts,
            "semantic {name}: {:?}",
            typed.diagnostics
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(&root)
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            case.accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
        for (compiler, output) in [("rustc", rustc), ("flux", flux)] {
            std::fs::write(
                dir.join(format!("{name}-{compiler}.stderr")),
                &output.stderr,
            )
            .unwrap();
            assert!(
                !String::from_utf8_lossy(&output.stderr).contains("panicked"),
                "{compiler} {name}: {:?}",
                output
            );
        }
    }
}
