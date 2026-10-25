//! Large Rust aggregates cross every call form without expanding all enum variants.
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn large_rust_arguments_match_rustc_off_and_on() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-large-rust-arguments-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["large_rust_arguments", "empty_enum_leaves"] {
    let source = root.join(format!("tests/run/{name}.rs"));
    let reference = dir.join("rustc");
    let output = Command::new("rustc").args(["--edition=2024", "-C", "panic=abort", "-C", "overflow-checks=off"]).arg(&source).arg("-o").arg(&reference).output().unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let expected = Command::new(&reference).output().unwrap();
    assert!(expected.status.success());
    for optimize in [false, true] {
        let executable = dir.join(if optimize { "flux-opt" } else { "flux" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimize { command.arg("-O"); }
        let output = command.output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let actual = Command::new(&executable).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
