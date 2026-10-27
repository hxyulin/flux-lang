//! Ordinary successful native fixtures, compiled independently by both compilers.
use super::process;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

pub fn success(output: &Output, label: &str) {
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn compare_fixture(name: &str) {
    if !super::cases::selected(name) {
        return;
    }
    if std::env::var_os("FLUX_TEST_NATIVE_SWEEP").is_some() {
        eprintln!("native comparison owned by tests/run: {name}");
        return;
    }
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = repo.join(format!("out/native-fixtures/{}/{name}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("create fixture artifacts");
    let source = repo.join(format!("tests/run/{name}.rs"));
    let limit = Duration::from_secs(60);
    let reference = directory.join("rustc");
    success(
        &process::run(
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
            &directory,
            "rustc-build",
            limit,
        )
        .completed(),
        name,
    );
    let expected =
        process::run(&mut Command::new(reference), &directory, "rustc-run", limit).completed();
    success(&expected, name);
    for optimized in [false, true] {
        let executable = directory.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        success(
            &process::run(
                &mut command,
                &directory,
                &format!("build-{optimized}"),
                limit,
            )
            .completed(),
            name,
        );
        let actual = process::run(
            &mut Command::new(executable),
            &directory,
            &format!("run-{optimized}"),
            limit,
        )
        .completed();
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "{name}, optimized={optimized}"
        );
        assert_eq!(
            actual.stdout,
            expected.stdout,
            "{name} stdout, optimized={optimized}; artifacts {}",
            directory.display()
        );
        assert_eq!(
            actual.stderr,
            expected.stderr,
            "{name} stderr, optimized={optimized}; artifacts {}",
            directory.display()
        );
    }
}
