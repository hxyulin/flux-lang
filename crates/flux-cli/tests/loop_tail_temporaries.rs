//! Loop-tail temporary lifetime and drop order in both editions.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod flib_support;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;
use std::process::Command;

#[test]
fn loop_tail_temporary_drop_order_matches_rustc() {
    let directory = Scratch::new(std::env::temp_dir().join("flux-loop-tail-native"));
    std::fs::create_dir_all(directory.path().join("src")).unwrap();
    std::fs::write(
        directory.path().join("src/main.rs"),
        include_str!("../../../tests/run/loop_tail_temporaries.rs"),
    )
    .unwrap();
    for edition in ["2021", "2024"] {
        std::fs::write(directory.path().join("Cargo.toml"), format!("[package]\nname=\"loop-tail-control\"\nversion=\"0.0.0\"\nedition=\"{edition}\"\n[workspace]\n")).unwrap();
        std::fs::write(
            directory.path().join("Cargo.lock"),
            "version=4\n[[package]]\nname=\"loop-tail-control\"\nversion=\"0.0.0\"\n",
        )
        .unwrap();
        let oracle = directory.path().join("oracle");
        let output = flib_support::run(
            Command::new("rustc")
                .current_dir(directory.path())
                .args(["--edition", edition, "src/main.rs", "-o"])
                .arg(&oracle),
            "loop-tail-rustc",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = Command::new(&oracle).output().unwrap();
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        for optimized in [false, true] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command.current_dir(directory.path()).args([
                "build",
                "--offline",
                "--manifest-path",
                "Cargo.toml",
                "-j1",
            ]);
            if optimized {
                command.arg("--release");
            }
            let output = flib_support::run(&mut command, "loop-tail-flux");
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let executable = directory.path().join(if optimized {
                "target/flux/release/loop-tail-control"
            } else {
                "target/flux/debug/loop-tail-control"
            });
            let actual = Command::new(&executable).output().unwrap();
            assert!(
                actual.status.success(),
                "{}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert_eq!(
                actual.stdout, expected.stdout,
                "edition={edition}, optimized={optimized}"
            );
        }
    }
}
