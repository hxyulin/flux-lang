//! Executes the current, unchanged SlicePool file through literal #[path].
//! This proves this source consumer, not the complete flux-arena crate.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

// File-backed output avoids a full pipe blocking a child before its timeout.
fn bounded(root: &Path, label: &str, command: &mut Command) -> Output {
    let stdout = root.join(format!("{label}.stdout"));
    let stderr = root.join(format!("{label}.stderr"));
    let mut child = command
        .stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap()))
        .env_remove("RUST_BACKTRACE")
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{label} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout).unwrap(),
        stderr: std::fs::read(stderr).unwrap(),
    }
}

fn compile(root: &Path, source: &Path, output: &Path, flux: bool, optimized: bool) {
    let mut command = Command::new(if flux {
        env!("CARGO_BIN_EXE_flux")
    } else {
        "rustc"
    });
    if flux {
        command.arg("build");
    } else {
        command.args([
            "--edition=2024",
            "-C",
            "overflow-checks=off",
            "-C",
            "panic=abort",
        ]);
    }
    command.arg(source).arg("-o").arg(output);
    if optimized {
        command.arg("-O");
    }
    let result = bounded(root, "compile", &mut command);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn actual_slice_pool_path_matches_rustc_in_both_modes() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let source = repo.join("crates/flux-arena/src/slice.rs");
    let original = std::fs::read(&source).unwrap();
    let root = repo.join("out/path-slice-pool");
    std::fs::create_dir_all(&root).unwrap();
    let prefix = format!("#[path = {:?}] mod slice;\n", source.to_str().unwrap());
    for (name, driver, succeeds) in [
        (
            "owned",
            include_str!("support/path_slice_pool_driver.rs"),
            true,
        ),
        // Safe Rust permits reusing a same-typed ID with a different pool;
        // the actual Index implementation must still enforce vector bounds.
        (
            "bounds",
            "fn main() { let mut a = slice::SlicePool::default(); let id = a.alloc([1, 2]); let b: slice::SlicePool<i32> = slice::SlicePool::default(); println!(\"{:?}\", &b[id]); }",
            false,
        ),
    ] {
        let driver_path = root.join(format!("{name}.rs"));
        std::fs::write(&driver_path, format!("{prefix}{driver}")).unwrap();
        let reference = root.join(format!("{name}-rustc"));
        // Rustc must accept each program before any executable is run.
        compile(&root, &driver_path, &reference, false, false);
        let expected = bounded(
            &root,
            &format!("{name}-rustc"),
            &mut Command::new(&reference),
        );
        assert_eq!(expected.status.success(), succeeds);
        if succeeds {
            assert_eq!(expected.stdout, b"slice-path 2 2 8\ndrops 1006\n");
        } else {
            assert!(
                String::from_utf8_lossy(&expected.stderr)
                    .contains("range end index 2 out of range for slice of length 0")
            );
        }
        for optimized in [false, true] {
            let executable = root.join(format!("{name}-flux-{optimized}"));
            compile(&root, &driver_path, &executable, true, optimized);
            let actual = bounded(
                &root,
                &format!("{name}-flux-{optimized}"),
                &mut Command::new(&executable),
            );
            assert_eq!(
                actual.status,
                expected.status,
                "{}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert_eq!(actual.stdout, expected.stdout);
            if succeeds {
                assert_eq!(actual.stderr, expected.stderr);
            }
            if !succeeds {
                assert!(
                    String::from_utf8_lossy(&actual.stderr)
                        .contains("range end index 2 out of range for slice of length 0")
                );
            }
        }
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}
