//! Executes a current embedded-library table entry, not all of flux-hir.
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

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn actual_library_table_and_dependency_data_match_rustc() {
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // Three directories below the repository: the production entry's literal
    // path stays unchanged, and the process cwd is deliberately elsewhere.
    let root = repo.join(format!(
        "out/include-str-source-{}/runtime",
        std::process::id()
    ));
    std::fs::create_dir_all(root.join("dep/src")).unwrap();
    let source = std::fs::read_to_string(repo.join("crates/flux-hir/src/source.rs")).unwrap();
    let entry = source
        .lines()
        .find(|line| line.contains("(\"hint.rs\", include_str!"))
        .unwrap()
        .trim()
        .trim_end_matches(',');
    let original = std::fs::read(repo.join("library/core/src/hint.rs")).unwrap();
    let bytes = "\u{feff}α\r\n\"quote\" \\ slash\0終\n";
    std::fs::write(root.join("data"), bytes).unwrap();
    std::fs::write(root.join("empty"), "").unwrap();
    std::fs::write(root.join("dep/src/data"), "dependency\r\n✓").unwrap();
    std::fs::write(
        root.join("dep/src/lib.rs"),
        "pub fn text() -> &'static str { include_str!(\"data\") }",
    )
    .unwrap();
    // Keep the bounded standalone manifests outside the compiler workspace.
    // Their targets retain the physical source paths above.
    let package =
        std::env::temp_dir().join(format!("flux-include-str-package-{}", std::process::id()));
    std::fs::create_dir_all(package.join("dep")).unwrap();
    std::fs::write(
        package.join("dep/Cargo.toml"),
        format!(
            "[package]\nname='included-dep'\nversion='0.1.0'\nedition='2024'\n[lib]\npath={:?}\n",
            root.join("dep/src/lib.rs").to_str().unwrap()
        ),
    )
    .unwrap();
    std::fs::write(package.join("Cargo.toml"), format!("[package]\nname='included-main'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='included-main'\npath={:?}\n[dependencies]\nincluded-dep={{path='dep'}}\n", root.join("driver.rs").to_str().unwrap())).unwrap();
    let driver = format!(
        r##"
const TABLE: &[(&str, &str)] = &[{entry}];
const TEXT: &str = include_str!("data");
static STATIC: &str = include_str!(r#"data"#,);
fn returned() -> &'static str {{ include_str!("data") }}
mod inline {{ pub fn text() -> &'static str {{ include_str!("data") }} }}
#[cfg(any())] const DISABLED: &str = include_str!("absent");
fn main() {{
    println!("{{}} {{:?}}", TABLE[0].0, TABLE[0].1.as_bytes());
    println!("{{:?}}", TEXT.as_bytes());
    println!("{{:?}}", STATIC.as_bytes());
    println!("{{:?}}", returned().as_bytes());
    println!("{{:?}}", inline::text().as_bytes());
    let empty = include_str!("empty");
    println!("{{:?}}", empty.as_bytes());
    println!("{{:?}}", included_dep::text().as_bytes());
}}
"##
    );
    let driver_path = root.join("driver.rs");
    std::fs::write(&driver_path, driver).unwrap();
    let lib = root.join("libincluded_dep.rlib");
    success(bounded(
        &root,
        "rustc-dep",
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-name=included_dep",
                "--crate-type=rlib",
            ])
            .arg(root.join("dep/src/lib.rs"))
            .arg("-o")
            .arg(&lib),
    ));
    let reference = root.join("rustc-main");
    success(bounded(
        &root,
        "rustc-main-compile",
        Command::new("rustc")
            .current_dir(std::env::temp_dir())
            .args([
                "--edition=2024",
                "-C",
                "overflow-checks=off",
                "-C",
                "panic=abort",
            ])
            .arg(&driver_path)
            .arg("--extern")
            .arg(format!("included_dep={}", lib.display()))
            .arg("-o")
            .arg(&reference),
    ));
    // All execution follows successful rustc compilation.
    let expected = success(bounded(
        &root,
        "rustc-main-run",
        &mut Command::new(reference),
    ));
    let expected_bytes = format!(
        "hint.rs {:?}\n{:?}\n{:?}\n{:?}\n{:?}\n[]\n{:?}\n",
        original,
        bytes.as_bytes(),
        bytes.as_bytes(),
        bytes.as_bytes(),
        bytes.as_bytes(),
        "dependency\r\n✓".as_bytes()
    );
    assert_eq!(
        expected.stdout,
        expected_bytes.as_bytes(),
        "rustc include_str byte oracle changed"
    );
    for optimized in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(std::env::temp_dir())
            .args(["package", "build", "--manifest-path"])
            .arg(package.join("Cargo.toml"));
        // The bounded package driver rejects compiler-wrapper/profile options
        // inherited from the host test build (as its existing tests do).
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.contains("RUSTFLAGS")
                || name.contains("RUSTDOCFLAGS")
                || name.starts_with("CARGO_BUILD_")
                || name.starts_with("CARGO_TARGET_")
                || name.starts_with("CARGO_PROFILE_")
                || name.starts_with("CARGO_UNSTABLE_")
                || name == "CARGO_INCREMENTAL"
                || matches!(&*name, "RUSTC_WRAPPER" | "RUSTC_WORKSPACE_WRAPPER")
            {
                command.env_remove(key);
            }
        }
        if optimized {
            command.arg("-O");
        }
        success(bounded(&root, "flux-compile", &mut command));
        let actual = success(bounded(
            &root,
            "flux-run",
            &mut Command::new(package.join("target/flux/included-main")),
        ));
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    assert_eq!(
        std::fs::read(repo.join("library/core/src/hint.rs")).unwrap(),
        original
    );
}
