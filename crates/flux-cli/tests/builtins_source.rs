//! Source-built compiler libraries remain covered even when a suite is staged.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod flib_support;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::Command;

fn normalized_stderr(bytes: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(bytes);
    let marker = "thread 'main' (";
    let mut output = String::new();
    let mut rest = stderr.as_ref();
    while let Some(at) = rest.find(marker) {
        let after = &rest[at + marker.len()..];
        let digits = after.bytes().take_while(u8::is_ascii_digit).count();
        output.push_str(&rest[..at + marker.len()]);
        if digits > 0 && after[digits..].starts_with(')') {
            output.push('N');
            rest = &after[digits..];
        } else {
            rest = after;
        }
    }
    output.push_str(rest);
    output
}

#[test]
fn forced_source_libraries_match_rustc_for_identity_panics_and_const_generics() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = scratch::Scratch::new(repository.join("out/builtins-source"));
    for fixture in ["any_type_id", "panic_track_caller", "const_generics"] {
        // Preserve the same physical source spelling in both compilers so
        // Location::caller and panic messages remain directly comparable.
        let source = format!("tests/run/{fixture}.rs");
        let reference = scratch.path().join(format!("{fixture}-rustc"));
        let compiled = flib_support::run(
            Command::new("rustc")
                .current_dir(&repository)
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
            "source-oracle-build",
        );
        assert!(
            compiled.status.success(),
            "{fixture}: {}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let expected = flib_support::run(
            Command::new(reference)
                .env("RUST_BACKTRACE", "0")
                .env("RUST_LIB_BACKTRACE", "0"),
            "source-oracle-run",
        );
        for optimized in [false, true] {
            let executable = scratch.path().join(format!("{fixture}-flux-{optimized}"));
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .current_dir(&repository)
                .env("FLUX_BUILTINS_SOURCE", "1")
                .env("FLUX_BUILTINS_TRACE", "1")
                .arg("build")
                .arg(&source)
                .arg("-o")
                .arg(&executable);
            if optimized {
                command.arg("-O");
            }
            let compiled = flib_support::run(&mut command, "forced-source-build");
            let stderr = String::from_utf8_lossy(&compiled.stderr);
            assert!(
                compiled.status.success(),
                "{fixture}, optimized={optimized}: {stderr}"
            );
            assert!(
                stderr.contains("flux builtins: source (forced)"),
                "{fixture}: {stderr}"
            );
            let actual = flib_support::run(
                Command::new(executable)
                    .env("RUST_BACKTRACE", "0")
                    .env("RUST_LIB_BACKTRACE", "0"),
                "forced-source-run",
            );
            assert_eq!(
                actual.status.code(),
                expected.status.code(),
                "{fixture}, optimized={optimized}"
            );
            assert_eq!(
                actual.status.signal(),
                expected.status.signal(),
                "{fixture}, optimized={optimized}"
            );
            assert_eq!(
                actual.stdout, expected.stdout,
                "{fixture}, optimized={optimized}"
            );
            assert_eq!(
                normalized_stderr(&actual.stderr),
                normalized_stderr(&expected.stderr),
                "{fixture}, optimized={optimized}"
            );
        }
    }
}
