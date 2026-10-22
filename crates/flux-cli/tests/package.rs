//! Offline manifest-driver tests. Cargo is never allowed to invoke rustc.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-package-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("src")).unwrap();
        let fixture = Self(root);
        fixture.write(
            "Cargo.toml",
            "[package]\nname = \"probe-bin\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        );
        fixture.write("src/main.rs", "fn main() { println!(\"probe=42\"); }");
        fixture.write(
            "deny-rustc",
            "#!/bin/sh\nprintf 'rustc invoked\\n' >> \"$RUSTC_CALL_LOG\"\nexit 86\n",
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                fixture.0.join("deny-rustc"),
                std::fs::Permissions::from_mode(0o755),
            )
            .unwrap();
        }
        fixture
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn command(&self, subcommand: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&self.0)
            .args(["package", subcommand, "--manifest-path", "Cargo.toml"])
            .env("CARGO_HOME", self.0.join("cargo-home"))
            .env("RUSTC", self.0.join("deny-rustc"))
            .env("RUSTC_WRAPPER", self.0.join("deny-rustc"))
            .env("RUSTC_WORKSPACE_WRAPPER", self.0.join("deny-rustc"))
            .env("RUSTC_CALL_LOG", self.0.join("rustc-calls"));
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.contains("RUSTFLAGS")
                || name.contains("RUSTDOCFLAGS")
                || name.starts_with("CARGO_BUILD_")
                || name.starts_with("CARGO_TARGET_")
                || name.starts_with("CARGO_PROFILE_")
                || name.starts_with("CARGO_UNSTABLE_")
                || name == "CARGO_INCREMENTAL"
            {
                command.env_remove(key);
            }
        }
        command
    }

    fn run(&self, subcommand: &str, args: &[&str]) -> Output {
        let output = self.command(subcommand).args(args).output().unwrap();
        assert!(!self.0.join("rustc-calls").exists(), "rustc was invoked");
        output
    }

    fn reject(&self, expected: &str) {
        let output = self.run("build", &[]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "expected {expected:?}: {stderr}");
        assert!(
            !stderr.contains("main.rs:"),
            "source compilation started: {stderr}"
        );
        assert!(!self.0.join("target/flux").exists());
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn offline_build_run_and_always_rebuild() {
    let fixture = Fixture::new();
    success(&fixture.run("build", &[]));
    let executable = fixture.0.join("target/flux/probe-bin");
    assert!(executable.exists());
    assert!(!fixture.0.join("target/debug").exists());
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"probe=42\n");
    fixture.write("src/main.rs", "fn main() { println!(\"changed\"); }");
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"changed\n");
    fixture.write("src/main.rs", "fn main() { let x: bool = 42; }");
    let output = fixture.run("build", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("main.rs:1:"));
    fixture.write("src/main.rs", "fn main( {");
    let output = fixture.run("build", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("main.rs:1:"));
    // Failed compilation must not run an executable left by an earlier build.
    let output = fixture.run("run", &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn custom_target_modules_arguments_and_exit_status() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='package-name'\nversion='0.1.0'\nedition='2024'\n[[bin]]\nname='actual-bin'\npath='app/entry.rs'\n");
    // Remove the inferred default binary, leaving only our explicit target.
    std::fs::remove_file(fixture.0.join("src/main.rs")).unwrap();
    fixture.write(
        "app/entry.rs",
        r#"
mod value;
unsafe extern "C" {
    fn _NSGetArgc() -> *mut i32;
    fn _NSGetArgv() -> *mut *mut *mut i8;
    fn printf(format: *const i8, ...) -> i32;
    fn exit(code: i32) -> !;
}
fn main() {
    println!("module={}", value::number());
    unsafe {
        let argc = *_NSGetArgc();
        let argv = *_NSGetArgv();
        printf(c"argc=%d arg=%s\n".as_ptr(), argc, *argv.add(1));
        exit(23);
    }
}
"#,
    );
    fixture.write("app/value.rs", "pub fn number() -> i32 { 42 }");
    let output = fixture.run(
        "run",
        &["--bin", "actual-bin", "--", "hello world", "--flag"],
    );
    assert_eq!(
        output.status.code(),
        Some(23),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"module=42\nargc=3 arg=hello world\n");
    assert!(fixture.0.join("target/flux/actual-bin").exists());
    assert!(!fixture.0.join("target/flux/entry").exists());
}

#[test]
fn unsupported_declarations_fail_before_compilation() {
    for (declaration, expected) in [
        ("[dependencies]\n", "dependencies"),
        ("[dev-dependencies]\n", "dev-dependencies"),
        ("[build-dependencies]\n", "build-dependencies"),
        ("[target.'cfg(unix)'.dependencies]\n", "target"),
        ("[features]\ndefault=[]\n", "features"),
        ("[profile.dev]\npanic='abort'\n", "profile"),
        ("[workspace]\n", "workspace"),
        ("[lib]\nproc-macro=true\n", "lib"),
        ("[[test]]\nname='custom'\npath='src/main.rs'\n", "test"),
        (
            "[[bin]]\nname='custom'\npath='src/main.rs'\nrequired-features=[]\n",
            "required-features",
        ),
    ] {
        let fixture = Fixture::new();
        let mut manifest = std::fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
        manifest.push_str(declaration);
        fixture.write("Cargo.toml", &manifest);
        fixture.write(
            "src/main.rs",
            "invalid source to prove validation precedes compilation",
        );
        fixture.reject(expected);
    }
    for option in [
        "build='custom.rs'",
        "build=false",
        "rust-version='1.99'",
        "default-run='probe-bin'",
    ] {
        let fixture = Fixture::new();
        let mut manifest = std::fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
        manifest.push_str(option);
        fixture.write("Cargo.toml", &manifest);
        fixture.reject("package option");
    }
}

#[test]
fn discovered_targets_configuration_and_ambiguity_are_rejected() {
    for (path, content, expected) in [
        ("build.rs", "fn main() {}", "build.rs"),
        ("src/lib.rs", "pub fn f() {}", "not a supported binary"),
        (
            "tests/extra.rs",
            "#[test] fn test() {}",
            "not a supported binary",
        ),
        (
            "examples/extra.rs",
            "fn main() {}",
            "not a supported binary",
        ),
        (
            ".cargo/config.toml",
            "[build]\ntarget='aarch64-apple-darwin'",
            "cargo configuration",
        ),
        (
            ".cargo/config",
            "[profile.dev]\nopt-level=2",
            "cargo configuration",
        ),
        (
            "src/bin/other.rs",
            "fn main() {}",
            "multiple binary targets",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.write(path, content);
        fixture.reject(expected);
    }
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[package]\nname='old'\nversion='0.1.0'\nedition='2021'\n",
    );
    fixture.reject("edition 2024");
    let fixture = Fixture::new();
    let output = fixture.run("build", &["--bin", "missing"]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("matching binary"));
    let output = fixture.run("build", &["--target", "aarch64-apple-darwin"]);
    assert!(String::from_utf8_lossy(&output.stderr).contains("unexpected argument"));
    let output = fixture
        .command("build")
        .env("CARGO_PROFILE_DEV_OPT_LEVEL", "3")
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("cargo environment option"));
}

#[test]
fn enclosing_workspace_is_rejected() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers=['member']\nresolver='3'\n",
    );
    fixture.write(
        "member/Cargo.toml",
        "[package]\nname='member'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("member/src/main.rs", "fn main() {} ");
    let mut command = fixture.command("build");
    command.current_dir(fixture.0.join("member"));
    let output = command.output().unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("workspaces are not supported"));
}

#[test]
fn malformed_manifest_is_reported() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package\n");
    fixture.reject("invalid TOML");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn selecting_an_inferred_binary_and_existing_file_commands() {
    let fixture = Fixture::new();
    fixture.write("src/bin/second.rs", "fn main() { println!(\"second\"); }");
    let output = fixture.run("run", &["--bin", "second"]);
    success(&output);
    assert_eq!(output.stdout, b"second\n");
    assert!(fixture.0.join("target/flux/second").exists());
    assert!(!fixture.0.join("target/flux/probe-bin").exists());
    let output = Command::new(env!("CARGO_BIN_EXE_flux"))
        .current_dir(&fixture.0)
        .args(["build", "src/main.rs", "-o", "legacy-output"])
        .output()
        .unwrap();
    success(&output);
    assert!(fixture.0.join("legacy-output").exists());
    let output = Command::new(env!("CARGO_BIN_EXE_flux"))
        .current_dir(&fixture.0)
        .args(["run", "src/main.rs"])
        .output()
        .unwrap();
    success(&output);
    assert_eq!(output.stdout, b"probe=42\n");
}

#[test]
fn cargo_home_and_ancestor_configuration_are_checked() {
    let fixture = Fixture::new();
    fixture.write(
        "cargo-home/config.toml",
        "[build]\nrustflags=['--cfg=custom']",
    );
    fixture.reject("cargo configuration");
    let fixture = Fixture::new();
    fixture.write(".cargo/config.toml", "[build]\ntarget='unsupported'");
    fixture.write(
        "nested/Cargo.toml",
        "[package]\nname='nested'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("nested/src/main.rs", "invalid source");
    let output = fixture
        .command("build")
        .current_dir(fixture.0.join("nested"))
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&output.stderr).contains("cargo configuration"));
    assert!(!fixture.0.join("nested/target/flux").exists());
}
