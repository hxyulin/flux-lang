#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
))]
mod support;
#[test]
fn pointer_array_apis_match_rustc() {
    support::oracle::compare_fixture("pointer_array_apis");
}

#[test]
fn zero_chunk_size_panics() {
    use std::process::Command;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(format!("out/pointer-array-zero/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for method in ["as_chunks", "as_chunks_mut", "as_rchunks"] {
        let source = root.join(format!("zero-{method}.rs"));
        std::fs::write(
            &source,
            format!("#![allow(unconditional_panic)] fn main() {{ let mut a = [1u8, 2]; let _ = a.{method}::<0>(); }}"),
        )
        .unwrap();
        for (label, compiler, optimized) in [
            ("rustc", "rustc", false),
            ("flux", env!("CARGO_BIN_EXE_flux"), false),
            ("flux-opt", env!("CARGO_BIN_EXE_flux"), true),
        ] {
            let executable = root.join(format!("{label}-{method}"));
            let mut command = Command::new(compiler);
            if label == "rustc" {
                command.args(["--edition=2024", "-C", "panic=abort"]);
            } else {
                command.arg("build");
            }
            command.arg(&source).arg("-o").arg(&executable);
            if optimized {
                command.arg("-O");
            }
            let built = command.output().unwrap();
            assert!(
                built.status.success(),
                "{label}: {}",
                String::from_utf8_lossy(&built.stderr)
            );
            let actual = Command::new(executable).output().unwrap();
            assert!(
                !actual.status.success(),
                "{label}: zero chunk size returned"
            );
            assert!(
                String::from_utf8_lossy(&actual.stderr).contains("chunk size must be non-zero"),
                "{label}: {:?}",
                actual.stderr
            );
        }
    }
}
