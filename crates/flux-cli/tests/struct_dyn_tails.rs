#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn struct_dyn_tails_match_rustc() {
    support::oracle::compare_fixture("struct_dyn_tails");
}

#[test]
fn struct_dyn_tail_cleanup_matches_rustc() {
    use std::{path::Path, process::Command, time::Duration};
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/struct-dyn-tail-unwind"),
    );
    let root = scratch.path();
    let source = root.join("case.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/struct_dyn_tails/unwind.rs.txt"),
    )
    .unwrap();
    let run = |command: &mut Command, label: &str| {
        let output =
            support::process::run(command, root, label, Duration::from_secs(60)).completed();
        support::oracle::success(&output, label);
        output
    };
    for optimized in [false, true] {
        let rustc = root.join(format!("rustc-{optimized}"));
        let mut reference = Command::new("rustc");
        reference
            .args(["--edition=2024", "-C", "panic=unwind"])
            .arg(&source)
            .arg("-o")
            .arg(&rustc);
        if optimized {
            reference.arg("-O");
        }
        run(&mut reference, "rustc-build");
        let expected = run(&mut Command::new(&rustc), "rustc-run");
        let flux = root.join(format!("flux-{optimized}"));
        let mut candidate = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        candidate
            .arg("build")
            .arg(&source)
            .args(["-C", "panic=unwind"])
            .arg("-o")
            .arg(&flux);
        if optimized {
            candidate.arg("-O");
        }
        run(&mut candidate, "flux-build");
        let actual = run(&mut Command::new(&flux), "flux-run");
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
