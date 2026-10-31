//! Real Unix file ownership and atomic replacement used by the Keel linker.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::Command;
use std::time::Duration;
#[test]
fn filesystem_output_matches_rustc_off_and_on() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = repo.join(format!("out/filesystem-output/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = repo.join("tests/run/filesystem_output.rs");
    let mut expected = None;
    for mode in ["rustc", "flux", "flux-opt"] {
        let executable = directory.join(mode);
        let mut command = if mode == "rustc" {
            let mut command = Command::new("rustc");
            command.args(["--edition=2024", "-C", "panic=abort"]);
            command
        } else {
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command.arg("build");
            if mode == "flux-opt" {
                command.arg("-O");
            }
            command
        };
        command.arg(&source).arg("-o").arg(&executable);
        let output = support::process::run(
            &mut command,
            &directory,
            &format!("build-{mode}"),
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, mode);
        let root = directory.join(format!("data-{mode}"));
        std::fs::create_dir_all(&root).unwrap();
        let output = support::process::run(
            Command::new("/bin/sh")
                .args([
                    "-c",
                    r#"ulimit -n 32 || exit 125; exec "$1""#,
                    "filesystem-test",
                ])
                .arg(executable)
                .env("FLUX_FS_SURFACE", &root),
            &directory,
            &format!("run-{mode}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, mode);
        if let Some((stdout, stderr)) = &expected {
            assert_eq!(&output.stdout, stdout, "{mode}");
            assert_eq!(&output.stderr, stderr, "{mode}");
        } else {
            expected = Some((output.stdout, output.stderr));
        }
    }
}
