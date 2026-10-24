//! Executes the unchanged StringTable implementation with Flux library APIs.
//! Other interner declarations and its original manifest are not compiled here.
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
fn unchanged_string_table_matches_rustc() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let source = repo.join("crates/flux-intern/src/lib.rs");
    let original = std::fs::read(&source).unwrap();
    let text = std::str::from_utf8(&original).unwrap();
    let start = text.find("struct StringTable {").unwrap();
    let end = start + text[start..].find("\n#[cfg(test)]").unwrap();
    let implementation = &text[start..end];
    let root = repo.join(format!("out/string-table-selfhost/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("unchanged-string-table.rs"), implementation).unwrap();
    let driver = root.join("consumer.rs");
    std::fs::write(&driver, format!("use std::ptr;\nuse std::sync::atomic::{{AtomicPtr, Ordering}};\nuse std::sync::OnceLock;\n{implementation}\n{}", include_str!("fixtures/string_table/consumer.rs.txt"))).unwrap();
    for flux in [false, true] {
        for optimized in [false, true] {
            let executable = root.join(format!("table-{flux}-{optimized}"));
            compile(&root, &driver, &executable, flux, optimized);
            let actual = bounded(&root, "execute", &mut Command::new(&executable));
            assert!(
                actual.status.success(),
                "{}",
                String::from_utf8_lossy(&actual.stderr)
            );
            assert_eq!(
                actual.stdout,
                "0 alpha\n1023 alpha\n1024 βeta\n3071 alpha\n3072 alpha\n4000 alpha\n".as_bytes()
            );
            assert!(actual.stderr.is_empty());
        }
    }
    assert_eq!(std::fs::read(source).unwrap(), original);
}
