//! Valid programs whose unrepresentable layouts or exhausted allocation abort.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(dir: &Path, command: &mut Command) -> Output {
    let stdout = dir.join("command.stdout");
    let stderr = dir.join("command.stderr");
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
        if start.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("boxed slice allocation command timed out: {command:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(stdout).unwrap(),
        stderr: std::fs::read(stderr).unwrap(),
    }
}

#[test]
fn invalid_layouts_and_allocation_failure_abort() {
    let dir = std::env::temp_dir().join(format!("flux-boxed-uninit-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, element, length, value, overflow) in [
        ("multiply", "u64", "usize::MAX", "7", true),
        ("isize_limit", "u8", "isize::MAX as usize + 1", "7", true),
        (
            "aligned_limit",
            "Aligned",
            "isize::MAX as usize / 64 + 1",
            "Aligned(7)",
            true,
        ),
        // Valid layout, but almost 8 EiB cannot fit the host's address space.
        (
            "exhausted",
            "Aligned",
            "isize::MAX as usize / 64 - 1",
            "Aligned(7)",
            false,
        ),
    ] {
        let source = dir.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            include_str!("fixtures/boxed_uninit_slice/allocation.rs.txt")
                .replace("ELEMENT", element)
                .replace("LENGTH", length)
                .replace("VALUE", value),
        )
        .unwrap();
        for mode in ["rustc", "flux", "flux-opt"] {
            let exe = dir.join(format!("{name}-{mode}"));
            let mut compile = if mode == "rustc" {
                let mut command = Command::new("rustc");
                command.args(["--edition=2024", "-C", "panic=abort"]);
                command
            } else {
                let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
                command.arg("build");
                if mode == "flux-opt" {
                    command.arg("-O");
                }
                command
            };
            compile.arg(&source).arg("-o").arg(&exe);
            let compiled = bounded(&dir, &mut compile);
            assert!(
                compiled.status.success(),
                "{name}-{mode}: {}",
                String::from_utf8_lossy(&compiled.stderr)
            );
            // Rustc accepts and executes each input first; no invalid Rust runs.
            let result = bounded(&dir, &mut Command::new(exe));
            assert_eq!(result.status.signal(), Some(6), "{name}-{mode}");
            assert!(result.stdout.is_empty(), "{name}-{mode}");
            if overflow {
                assert!(
                    String::from_utf8_lossy(&result.stderr).contains("capacity overflow"),
                    "{name}-{mode}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
            }
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
