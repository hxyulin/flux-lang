//! Compile-only oracles: contradictory assumptions must not pick a wrong output.
use std::path::Path;
use std::process::Command;

#[test]
fn projection_proof_equalities_match_rustc() {
    let exact = include_str!("fixtures/proof_equalities/conflicting.rs.txt");
    let cases = [
        ("exact", exact.to_owned(), false),
        (
            "explicit_output",
            exact
                .replace("-> u16\nwhere", "-> u8\nwhere")
                .replace("(h: T)", "(h: T::Part)")
                .replace("h.part().get()", "h.get()"),
            true,
        ),
        (
            "generic_output",
            exact
                .replace("trait Q: P<A = u8>", "trait Q<X>: P<A = X>")
                .replace("T::Part: Q,", "T::Part: Q<u8>,")
                .replace("-> u16\nwhere", "-> u8\nwhere")
                .replace("(h: T)", "(h: T::Part)")
                .replace("h.part().get()", "h.get()"),
            true,
        ),
        ("matching", exact.replace("P<A = u8>", "P<A = u16>"), true),
        (
            "reordered",
            exact
                .replace("T: H", "T")
                .replace("T::Part: Q,", "T::Part: Q, T: H,"),
            false,
        ),
        (
            "transitive",
            exact.replace(
                "trait Q: P<A = u8> {}",
                "trait R: P<A = u8> {} trait Q: R {}",
            ),
            false,
        ),
        (
            "generic",
            exact
                .replace("trait Q: P<A = u8>", "trait Q<X>: P<A = X>")
                .replace("T::Part: Q,", "T::Part: Q<u8>,"),
            false,
        ),
        (
            "direct",
            exact.replace("T::Part: Q,", "T::Part: P<A = u8>,"),
            false,
        ),
        (
            "direct_output",
            exact
                .replace("T::Part: Q,", "T::Part: P<A = u8>,")
                .replace("-> u16\nwhere", "-> u8\nwhere")
                .replace("(h: T)", "(h: T::Part)")
                .replace("h.part().get()", "h.get()"),
            true,
        ),
        (
            "matching_wrong_return",
            exact
                .replace("P<A = u8>", "P<A = u16>")
                .replace("-> u16\nwhere", "-> u8\nwhere"),
            false,
        ),
        (
            "runtime",
            include_str!("../../../tests/run/proof_equalities.rs").to_owned(),
            true,
        ),
        (
            "iterator",
            include_str!("../../../tests/run/iterator_projection_bounds.rs").to_owned(),
            true,
        ),
    ];
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-proof-equalities-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in cases {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, &source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&file, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepts,
            "semantic {name}: {:?}",
            typed.diagnostics
        );
        if !accepts {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message.contains("mismatched types")),
                "{name}: {:?}",
                typed.diagnostics
            );
        }
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(&root)
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        for (compiler, output) in [("rustc", rustc), ("flux", flux)] {
            let diagnostics = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.success(),
                accepts,
                "{compiler} {name}: {diagnostics}"
            );
            if !accepts {
                assert!(
                    diagnostics.contains("mismatched types"),
                    "{compiler} {name}: {diagnostics}"
                );
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
