//! Paths and physical source I/O compared with the host standard library.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(30)).completed()
}

#[test]
fn paths_and_physical_files_match_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/path-filesystem/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let fixtures = root.join("files");
    std::fs::create_dir_all(fixtures.join("directory")).unwrap();
    std::fs::write(fixtures.join("text"), "hello β\r\n").unwrap();
    std::fs::write(fixtures.join("binary"), b"a\xffb").unwrap();
    let link = fixtures.join("link");
    if !link.exists() {
        std::os::unix::fs::symlink("text", &link).unwrap();
    }
    let link = fixtures.join("loop");
    if std::fs::symlink_metadata(&link).is_err() {
        std::os::unix::fs::symlink("loop", &link).unwrap();
    }
    let fixtures = fixtures.canonicalize().unwrap();
    let source = repo.join("tests/run/path_filesystem.rs");
    let reference = root.join("reference");
    let built = bounded(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "rustc-build",
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let expected = bounded(
        Command::new(&reference).env("FLUX_PATH_FIXTURE", &fixtures),
        &root,
        "rustc-run",
    );
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let built = bounded(&mut command, &root, &format!("build-{optimized}"));
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let actual = bounded(
            Command::new(&executable).env("FLUX_PATH_FIXTURE", &fixtures),
            &root,
            &format!("run-{optimized}"),
        );
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
