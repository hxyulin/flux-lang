#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::Path;
use std::process::Command;

#[test]
fn projected_pointer_regions_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        "out/projected-pointer-regions/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let source = root.join("tests/run/projected_pointer_regions.rs");
    for optimized in [false, true] {
        let mut results = Vec::new();
        for compiler in ["rustc", "flux"] {
            let executable = dir.join(format!("{compiler}-{optimized}"));
            let mut command = if compiler == "rustc" {
                let mut command = Command::new("rustc");
                command.args([
                    "--edition=2024",
                    "-C",
                    "panic=abort",
                    "-C",
                    "overflow-checks=off",
                ]);
                command
            } else {
                let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
                command.arg("build");
                command
            };
            if optimized {
                command.arg("-O");
            }
            let output = command
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(executable).output().unwrap();
            assert!(
                output.status.success(),
                "{compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            results.push((output.status.code(), output.stdout, output.stderr));
        }
        assert_eq!(results[0], results[1], "optimized={optimized}");
    }
}
