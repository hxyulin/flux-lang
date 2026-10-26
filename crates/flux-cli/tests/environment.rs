//! Native environment byte handling is compared with the standard library.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::os::unix::ffi::OsStringExt;
use std::process::Command;

fn run(path: &std::path::Path) -> std::process::Output {
    Command::new(path)
        .env("FLUX_SELFHOST_ENV_TEXT", "hello β\n")
        .env(
            "FLUX_SELFHOST_ENV_BYTES",
            std::ffi::OsString::from_vec(b"a\xff\xce\xb2\xfe".to_vec()),
        )
        .env_remove("FLUX_SELFHOST_ENV_MISSING")
        .output()
        .unwrap()
}

#[test]
fn environment_queries_match_rustc() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = repo.join("tests/run/environment.rs");
    let root = repo.join(format!("out/environment/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let reference = root.join("reference");
    let result = Command::new("rustc")
        .args(["--edition=2024", "-C", "panic=abort"])
        .arg(&source)
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let expected = run(&reference);
    assert!(expected.status.success());
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let result = command.output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let actual = run(&executable);
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
