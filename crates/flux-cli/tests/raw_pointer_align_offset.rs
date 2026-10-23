//! Focused panic and compile-only boundaries, separate from runtime comparison.
use std::path::Path;
use std::process::Command;

#[test]
fn invalid_alignment_panics_like_rustc() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let dir = std::env::temp_dir().join(format!("flux-align-offset-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for mutable in [false, true] {
        for align in [0usize, 3, 6, usize::MAX] {
            let pointer = if mutable { "*mut u32" } else { "*const u32" };
            let file = dir.join("panic.rs");
            std::fs::write(
                &file,
                format!(
                    "fn check(a: usize) {{ println!(\"{{}}\", (1usize as {pointer}).align_offset(a)); }} fn main() {{ check({align}); }}"
                ),
            )
            .unwrap();
            let reference = dir.join("rustc");
            let result = Command::new("rustc")
                .args(["--edition=2024", "-Cpanic=abort"])
                .arg(&file)
                .arg("-o")
                .arg(&reference)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
            let expected = Command::new(&reference).output().unwrap();
            assert!(!expected.status.success());
            let message = "align_offset: align is not a power-of-two";
            assert!(String::from_utf8_lossy(&expected.stderr).contains(message));
            for optimize in [false, true] {
                let executable = dir.join("flux");
                let mut compiler = Command::new(env!("CARGO_BIN_EXE_flux"));
                compiler.arg("build").arg(&file).arg("-o").arg(&executable);
                if optimize {
                    compiler.arg("-O");
                }
                let result = compiler.output().unwrap();
                assert!(
                    result.status.success(),
                    "{}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let actual = Command::new(&executable).output().unwrap();
                assert_eq!(actual.status, expected.status);
                assert_eq!(actual.stdout, expected.stdout);
                // Library source locations differ; compare the actual panic contract.
                assert!(String::from_utf8_lossy(&actual.stderr).contains(message));
            }
        }
    }
}

#[test]
fn const_and_unsized_boundaries_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-align-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [
        (
            "constant",
            "const N: usize = (1usize as *const u8).align_offset(8); fn main() {}",
        ),
        (
            "const_fn",
            "const fn f(p: *mut u32) -> usize { p.align_offset(8) } fn main() {}",
        ),
        (
            "unsized",
            "fn f(p: *const [u8]) -> usize { p.align_offset(8) } fn main() {}",
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert!(!rustc.status.success());
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert!(!flux.status.success(), "accepted {name}");
        let diagnostics = String::from_utf8_lossy(&flux.stderr);
        let expected = if name == "unsized" {
            "cannot be known at compilation time"
        } else {
            "cannot call non-const method"
        };
        assert!(diagnostics.contains(expected), "{name}: {diagnostics}");
    }
}

#[test]
fn unchanged_dropless_has_no_align_offset_error() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("flux-dropless-align-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("probe.rs");
    std::fs::write(
        &file,
        format!(
            "#[path = {:?}] mod dropless; fn main() {{}}",
            root.join("crates/flux-arena/src/dropless.rs")
        ),
    )
    .unwrap();
    let rustc = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&file)
        .arg("-o")
        .arg(dir.join("probe.rmeta"))
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("mir")
        .arg(&file)
        .output()
        .unwrap();
    let diagnostics = String::from_utf8_lossy(&flux.stderr);
    assert!(!diagnostics.contains("align_offset"), "{diagnostics}");
    if !flux.status.success() {
        assert!(diagnostics.contains("NonNull"), "{diagnostics}");
    }
}
