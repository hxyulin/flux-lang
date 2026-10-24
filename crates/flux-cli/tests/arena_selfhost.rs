//! Executes the unchanged complete arena source graph, without its Cargo manifest.
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
fn unchanged_arena_root_matches_rustc() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let source = repo.join("crates/flux-arena/src/lib.rs");
    let originals: Vec<_> = [
        "lib.rs",
        "dropless.rs",
        "shared_slice.rs",
        "slice.rs",
        "static_slice.rs",
        "typed.rs",
    ]
    .into_iter()
    .map(|name| {
        let path = source.parent().unwrap().join(name);
        let bytes = std::fs::read(&path).unwrap();
        (path, bytes)
    })
    .collect();
    let root = repo.join(format!("out/arena-selfhost/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let driver = root.join("consumer.rs");
    std::fs::write(
        &driver,
        format!(
            "#[path = {:?}] mod arena;\n{}",
            source.to_str().unwrap(),
            include_str!("fixtures/arena/consumer.rs.txt")
        ),
    )
    .unwrap();
    for flux in [false, true] {
        for optimized in [false, true] {
            let executable = root.join(format!("arena-{flux}-{optimized}"));
            compile(&root, &driver, &executable, flux, optimized);
            let output = bounded(&root, "execute", &mut Command::new(&executable));
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(
                output.stdout,
                b"arena 14 22 map 10\nlayered tail base!\nshared 3 static 3 bump 9\n"
            );
            assert!(output.stderr.is_empty());
        }
    }
    for (path, bytes) in originals {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
}
