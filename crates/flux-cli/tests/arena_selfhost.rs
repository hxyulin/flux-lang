//! Executes the unchanged complete arena source graph, without its Cargo manifest.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

// File-backed output avoids a full pipe blocking a child before its timeout.
fn bounded(root: &Path, label: &str, command: &mut Command) -> Output {
    command.env_remove("RUST_BACKTRACE");
    support::process::run(command, root, label, Duration::from_secs(60)).completed()
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
    let scratch = support::scratch::Scratch::new(repo.join("out/arena-selfhost"));
    let root = scratch.path().to_path_buf();
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
