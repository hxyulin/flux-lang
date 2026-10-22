//! Offline manifest-driver tests. Cargo is never allowed to invoke rustc.
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "flux-edition-test-{}-{}",
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

fn dependency(fixture: &Fixture, source: &str) {
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndep={path='dep'}\n");
    fixture.write("dep/Cargo.toml", "[package]\nname='dep'\nversion='0.1.0'\nedition='2021'\nbuild=false\nrust-version='1.71'\nautolib=false\nautobins=false\nautotests=false\nautobenches=false\nautoexamples=false\n[package.metadata.docs.rs]\ntargets=['x86_64-unknown-linux-gnu']\n[lib]\nname='dep'\npath='src/lib.rs'\n[[test]]\nname='unused'\npath='tests/unused.rs'\n[[bench]]\nname='unused'\npath='benches/unused.rs'\nharness=false\n[dev-dependencies.not-cached]\nversion='99'\n[target.'cfg(not(miri))'.dev-dependencies.also-not-cached]\nversion='99'\n");
    fixture.write("dep/src/lib.rs", source);
    fixture.write("dep/tests/unused.rs", "invalid unselected test source");
    fixture.write(
        "dep/benches/unused.rs",
        "invalid unselected benchmark source",
    );
    fixture.write("dep/build.rs", "invalid disabled build script");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn mixed_editions_normalized_library_unit_matches_rustc() {
    let fixture = Fixture::new();
    let source = include_str!("fixtures/edition2021/lib.rs.txt");
    dependency(&fixture, source);
    fixture.write("dep/src/child.rs", "pub fn gen() -> u32 { 42 }");
    fixture.write(
        "src/main.rs",
        "fn main() { println!(\"{}\", dep::r#gen()); }",
    );
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"42\n");
    let lib = fixture.0.join("libdep.rlib");
    let status = Command::new("rustc")
        .args(["--edition=2021", "--crate-type=rlib", "--crate-name=dep"])
        .arg(fixture.0.join("dep/src/lib.rs"))
        .arg("-o")
        .arg(&lib)
        .status()
        .unwrap();
    assert!(status.success());
    let exe = fixture.0.join("rustc-main");
    let status = Command::new("rustc")
        .args(["--edition=2024", "--extern"])
        .arg(format!("dep={}", lib.display()))
        .arg(fixture.0.join("src/main.rs"))
        .arg("-o")
        .arg(&exe)
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(Command::new(exe).output().unwrap().stdout, output.stdout);
    let output = Command::new("rustc")
        .args(["--edition=2024", "--crate-type=rlib"])
        .arg(fixture.0.join("dep/src/lib.rs"))
        .arg("-o")
        .arg(&lib)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reserved keyword `gen`"));
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn sensitive_2021_constructs_fail_explicitly() {
    for (source, expected) in [
        (
            "pub fn f() { if let Some(_) = Some(1) {} }",
            "if-let, while-let and let chains in edition 2021",
        ),
        (
            "pub fn f() { if let Some(_) = Some(1) && true {} }",
            "let chains in edition 2021",
        ),
        (
            "pub fn f() -> impl Copy { 1u32 }",
            "impl Trait in edition 2021",
        ),
        (
            "macro_rules! f { ($e:expr) => { $e }; } pub fn f() {}",
            "macro definitions in edition 2021",
        ),
        (
            "struct D; impl Drop for D { fn drop(&mut self) {} } fn make() -> D { D } pub fn f() -> bool { let _d = D; { let _ = &make(); true } }",
            "values requiring drop in edition 2021",
        ),
        ("pub fn f() { let [mut x] = &[1u32]; }", "binding modifiers"),
        (
            "fn take<T>(_: T) {} pub fn f() { take(loop {}); }",
            "never type fallback in edition 2021",
        ),
    ] {
        let fixture = Fixture::new();
        dependency(&fixture, source);
        fixture.write("src/main.rs", "fn main() {}");
        let output = fixture.run("build", &[]);
        assert!(!output.status.success(), "accepted {source}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "expected {expected}: {stderr}");
    }
}

#[test]
fn active_dependency_configuration_is_still_rejected() {
    for (declaration, expected) in [
        ("\n[features]\ndefault=[]\n", "features"),
        ("\n[build-dependencies]\ncc='1'\n", "build-dependencies"),
        (
            "\n[target.'cfg(unix)'.dependencies]\ncc='1'\n",
            "dependencies",
        ),
        ("\n[workspace]\n", "workspace"),
    ] {
        let fixture = Fixture::new();
        dependency(&fixture, "pub fn f() {}");
        let manifest = std::fs::read_to_string(fixture.0.join("dep/Cargo.toml")).unwrap();
        fixture.write("dep/Cargo.toml", &(manifest + declaration));
        fixture.reject(expected);
    }
    for value in ["true", "'build.rs'"] {
        let fixture = Fixture::new();
        dependency(&fixture, "pub fn f() {}");
        let manifest = std::fs::read_to_string(fixture.0.join("dep/Cargo.toml")).unwrap();
        fixture.write(
            "dep/Cargo.toml",
            &manifest.replace("build=false", &format!("build={value}")),
        );
        fixture.reject("build scripts");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn tail_temporary_drop_order_is_not_silently_compiled_as_2024() {
    let fixture = Fixture::new();
    let source = include_str!("fixtures/edition2021/drop_order.rs");
    dependency(&fixture, source);
    fixture.write("src/main.rs", "fn main() {}");
    let mut observed = Vec::new();
    for edition in ["2021", "2024"] {
        let exe = fixture.0.join(format!("rustc-{edition}"));
        let status = Command::new("rustc")
            .arg(format!("--edition={edition}"))
            .arg(fixture.0.join("dep/src/lib.rs"))
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap();
        assert!(status.success());
        observed.push(Command::new(exe).output().unwrap().stdout);
    }
    assert_eq!(observed[0], b"drop=1\ndrop=2\nvalue=2\n");
    assert_eq!(observed[1], b"drop=2\ndrop=1\nvalue=2\n");
    let output = fixture.run("build", &[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("values requiring drop in edition 2021")
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn if_let_temporary_drop_order_is_not_silently_compiled_as_2024() {
    let fixture = Fixture::new();
    dependency(
        &fixture,
        include_str!("fixtures/edition2021/if_let_drop_order.rs"),
    );
    fixture.write("src/main.rs", "fn main() {}");
    let mut observed = Vec::new();
    for edition in ["2021", "2024"] {
        let exe = fixture.0.join(format!("rustc-{edition}"));
        let status = Command::new("rustc")
            .arg(format!("--edition={edition}"))
            .arg(fixture.0.join("dep/src/lib.rs"))
            .arg("-o")
            .arg(&exe)
            .status()
            .unwrap();
        assert!(status.success());
        observed.push(Command::new(exe).output().unwrap().stdout);
    }
    assert_eq!(observed[0], b"else\ndrop\n");
    assert_eq!(observed[1], b"drop\nelse\n");
    let output = fixture.run("build", &[]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("if-let, while-let and let chains in edition 2021")
    );
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn edition_sensitive_syntax_and_lifetime_rejections_match_rustc_boundaries() {
    for (source, expected, accepts_2021, accepts_2024) in [
        (
            "pub fn f() { if let Some(_) = Some(1) && true {} }",
            "let chains in edition 2021",
            false,
            true,
        ),
        (
            "pub fn f(x: &u32) -> impl Copy { x }",
            "impl Trait in edition 2021",
            false,
            true,
        ),
        (
            "pub fn f() { let [mut x] = &[1u32]; }",
            "binding modifiers",
            true,
            false,
        ),
    ] {
        let fixture = Fixture::new();
        dependency(&fixture, source);
        fixture.write("src/main.rs", "fn main() {}");
        for (edition, accepts) in [("2021", accepts_2021), ("2024", accepts_2024)] {
            let output = Command::new("rustc")
                .arg(format!("--edition={edition}"))
                .args(["--crate-type=rlib", "--emit=metadata"])
                .arg(fixture.0.join("dep/src/lib.rs"))
                .arg("-o")
                .arg(fixture.0.join("rustc.rmeta"))
                .output()
                .unwrap();
            assert_eq!(
                output.status.success(),
                accepts,
                "edition {edition}: {source}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let output = fixture.run("build", &[]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains(expected));
    }
}
