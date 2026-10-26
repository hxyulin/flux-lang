#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use std::path::Path;
use std::process::Command;

#[test]
fn bumpalo_library_prerequisites_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join(format!(
        "out/bumpalo-library-prerequisites/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let source = root.join("tests/run/bumpalo_library_prerequisites.rs");
    for optimized in [false, true] {
        let mut results = Vec::new();
        for compiler in ["rustc", "flux"] {
            let executable = dir.join(format!("{compiler}-{optimized}"));
            let mut command = if compiler == "rustc" {
                let mut c = Command::new("rustc");
                c.args(["--edition=2024", "-C", "panic=abort"]);
                c
            } else {
                let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
                c.arg("build");
                c
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
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(executable).output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            results.push((output.stdout, output.stderr));
        }
        assert_eq!(results[0], results[1]);
    }
}
