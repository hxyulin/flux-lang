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
    for option in ["build='custom.rs'", "default-run='probe-bin'"] {
        let fixture = Fixture::new();
        let mut manifest = std::fs::read_to_string(fixture.0.join("Cargo.toml")).unwrap();
        manifest.push_str(option);
        fixture.write("Cargo.toml", &manifest);
        fixture.reject(if option.starts_with("build=") {
            "build scripts"
        } else {
            "package option"
        });
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
        "[package]\nname='old'\nversion='0.1.0'\nedition='2018'\n",
    );
    fixture.reject("edition 2021 or 2024");
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

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn source_dependencies_preserve_crate_identity_and_rebuild() {
    let fixture = Fixture::new();
    fixture.write("leaf/Cargo.toml", "[package]\nname='leaf-package'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='actual_leaf'\npath='code/root.rs'\n");
    fixture.write("leaf/code/root.rs", "mod implementation; pub use implementation::{Value, make}; pub fn apply<T, F: Fn(T) -> T>(v: T, f: F) -> T { f(v) }");
    fixture.write(
        "leaf/code/implementation.rs",
        "pub struct Value(pub i32); pub fn make() -> Value { Value(17) }",
    );
    fixture.write("middle/Cargo.toml", "[package]\nname='middle'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ninside={path='../leaf',package='leaf-package'}\n");
    fixture.write("middle/src/lib.rs", "pub use inside::Value; pub fn value() -> Value { inside::apply(inside::make(), |v| Value(v.0 + 5)) }");
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nrenamed-leaf={path='leaf',package='leaf-package'}\nmiddle={path='middle'}\n");
    fixture.write("src/main.rs", "extern crate middle; fn show(v: renamed_leaf::Value) { println!(\"dependency={}\", v.0); } fn main() { show(middle::value()); }");
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"dependency=22\n");
    fixture.write(
        "leaf/code/implementation.rs",
        "pub struct Value(pub i32); pub fn make() -> Value { Value(31) }",
    );
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"dependency=36\n");
    // A failed dependency rebuild must not execute the old successful binary.
    fixture.write(
        "leaf/code/implementation.rs",
        "pub struct Value(pub i32); pub fn make() -> Value { let n: bool = 3; Value(0) }",
    );
    let output = fixture.run("run", &[]);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("implementation.rs"));
}

#[test]
fn unsupported_path_dependency_graphs_are_rejected() {
    for spec in [
        "'1.0'",
        "{git='https://example.invalid/lib'}",
        "{path='dep',features=[]}",
        "{path='dep',optional=true}",
        "{path='dep',version='0.1'}",
    ] {
        let fixture = Fixture::new();
        fixture.write("Cargo.toml", &format!("[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndep={spec}\n"));
        fixture.reject(if spec.starts_with("{path") || spec.starts_with("{git") {
            "dependency option"
        } else {
            "local path"
        });
    }
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndep={path='dep'}\n");
    fixture.write("dep/Cargo.toml", "[package]\nname='dep'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nprobe-bin={path='..'}\n");
    fixture.write("dep/src/lib.rs", "pub fn f() {}");
    fixture.reject("cyclic path dependencies");
    fixture.write(
        "dep/Cargo.toml",
        "[package]\nname='dep'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    fixture.reject("library option");
    fixture.write(
        "dep/Cargo.toml",
        "[package]\nname='dep'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("dep/build.rs", "fn main() {}");
    fixture.reject("build.rs");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unchanged_compiler_source_builds_as_a_dependency() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ncompiler-rng={path='rng'}\n");
    fixture.write("rng/Cargo.toml", "[package]\nname='compiler-rng'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='choice_tape'\n");
    fixture.write(
        "rng/src/lib.rs",
        include_str!("../../flux-difftest/src/rng.rs"),
    );
    // An unrenamed dependency uses its library target name, not package name.
    fixture.write("src/main.rs", "fn main() { let mut source = choice_tape::Source::replay(vec![3, 5]); println!(\"{} {} {}\", source.below(10), source.below(10), source.below(10)); }");
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"3 5 0\n");
    assert!(!fixture.0.join("rng/target").exists());
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn identical_library_names_have_distinct_native_symbols() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='same'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nfirst={path='first',package='first-package'}\nsecond={path='second',package='second-package'}\n");
    for (directory, number) in [("first", 14), ("second", 24)] {
        fixture.write(&format!("{directory}/Cargo.toml"), &format!("[package]\nname='{directory}-package'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='same'\n"));
        fixture.write(
            &format!("{directory}/src/lib.rs"),
            &format!("pub static VALUE: i32 = {number}; pub fn value() -> i32 {{ VALUE }}"),
        );
    }
    fixture.write("src/main.rs", "static VALUE: i32 = 94; fn value() -> i32 { VALUE } fn main() { println!(\"{} {} {}\", first::value(), second::value(), value()); }");
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"14 24 94\n");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn dependency_macros_keep_definition_crate_across_reexports() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nbridge={path='bridge'}\n");
    fixture.write(
        "leaf/Cargo.toml",
        "[package]\nname='original'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("leaf/src/lib.rs", r#"
pub fn answer() -> i32 { 42 }
pub struct Value(pub i32);
#[macro_export] macro_rules! helper { () => { $crate::answer() } }
#[macro_export] macro_rules! value { () => { $crate::helper!() } }
#[macro_export] macro_rules! ty { () => { $crate::Value } }
#[macro_export] macro_rules! make { ($name:ident) => { fn $name() -> $crate::Value { $crate::Value($crate::value!()) } } }
#[macro_export] macro_rules! forward { ($e:expr) => { $e } }
#[macro_export] macro_rules! forwarded { () => { $crate::forward!($crate::answer()) } }
#[macro_export] macro_rules! install { () => { macro_rules! installed { () => { $crate::answer() }; } } }
#[macro_export] macro_rules! classify { (crate) => { 1 }; ($t:tt) => { 2 }; }
#[macro_export] macro_rules! classified { () => { $crate::classify!($crate) } }
"#);
    fixture.write("bridge/Cargo.toml", "[package]\nname='bridge'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nrenamed={path='../leaf',package='original'}\n");
    fixture.write(
        "bridge/src/lib.rs",
        "pub use renamed::{value, ty, make, forwarded, install, classified};",
    );
    fixture.write(
        "src/main.rs",
        r#"
use bridge::make;
make!(created);
bridge::install!();
mod original { pub fn answer() -> i32 { 999 } }
fn main() {
    let value: bridge::ty!() = created();
    println!("{} {} {} {} {} {}", value.0, bridge::value!(), bridge::forwarded!(), installed!(), bridge::classified!(), original::answer());
}
"#,
    );
    for args in [&[][..], &["-O"][..]] {
        let output = fixture.run("run", args);
        success(&output);
        assert_eq!(output.stdout, b"42 42 42 42 2 999\n");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn no_std_path_library_can_opt_in_to_alloc_or_std() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nnostd={path='nostd'}\n");
    fixture.write(
        "nostd/Cargo.toml",
        "[package]\nname='nostd'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("nostd/src/lib.rs", "#![no_std] extern crate alloc; extern crate std; use alloc::vec::Vec; #[rustfmt::skip] pub fn values() -> Vec<i32> { let mut v = alloc::vec![3, 5]; v.push(7); std::println!(\"library {}\", v.len()); v }");
    fixture.write(
        "src/main.rs",
        "fn main() { let values = nostd::values(); println!(\"app {:?}\", values); }",
    );
    let output = fixture.run("run", &[]);
    success(&output);
    assert_eq!(output.stdout, b"library 3\napp [3, 5, 7]\n");
}

fn copy_source_package(source: &std::path::Path, target: &std::path::Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_source_package(&entry.path(), &destination);
        } else {
            std::fs::copy(entry.path(), destination).unwrap();
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unicode_ident_sources_match_rustc_on_every_unicode_scalar() {
    // Locate the version Cargo already built for this workspace. The reference
    // compilation below is an explicit oracle, separate from Flux invocations
    // whose rejecting wrappers must continue to record zero compiler calls.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new("cargo")
        .current_dir(&root)
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--format-version",
            "1",
            "--filter-platform",
            "aarch64-apple-darwin",
        ])
        .output()
        .unwrap();
    success(&output);
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "unicode-ident")
        .unwrap();
    let manifest = std::path::Path::new(package["manifest_path"].as_str().unwrap());
    let fixture = Fixture::new();
    // Preserve the original package and sources. The explicit wrapper selects
    // edition 2024; loading the original edition-2021 manifest is a later gate.
    copy_source_package(
        manifest.parent().unwrap(),
        &fixture.0.join("unicode-ident-src"),
    );
    fixture.write("unicode-ident/Cargo.toml", "[package]\nname='unicode-ident'\nversion='0.1.0'\nedition='2024'\n[lib]\npath='../unicode-ident-src/src/lib.rs'\n");
    fixture.write("Cargo.toml", "[package]\nname='probe-bin'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nunicode-ident={path='unicode-ident'}\n");
    fixture.write("src/main.rs", r#"
fn main() {
    let mut starts = 0u64;
    let mut continues = 0u64;
    let mut digest = 0u64;
    for n in 0u32..=0x10ffff {
        if let Some(ch) = char::from_u32(n) {
            let start = unicode_ident::is_xid_start(ch);
            let cont = unicode_ident::is_xid_continue(ch);
            starts += start as u64;
            continues += cont as u64;
            digest = digest.rotate_left(7) ^ ((n as u64) << 2) ^ ((start as u64) << 1) ^ cont as u64;
        }
    }
    println!("{} {} {} {:?}", starts, continues, digest, unicode_ident::UNICODE_VERSION);
}
"#);
    let library = fixture.0.join("reference.rlib");
    let output = Command::new("rustc")
        .args([
            "--crate-type",
            "rlib",
            "--crate-name",
            "unicode_ident",
            "--edition",
            "2024",
        ])
        .arg(fixture.0.join("unicode-ident-src/src/lib.rs"))
        .arg("-o")
        .arg(&library)
        .output()
        .unwrap();
    success(&output);
    // rustc requires the conventional library filename for --extern paths.
    let named_library = fixture.0.join("libunicode_ident.rlib");
    std::fs::rename(library, &named_library).unwrap();
    let reference = fixture.0.join("reference");
    let output = Command::new("rustc")
        .args([
            "--edition",
            "2024",
            "-C",
            "panic=abort",
            "-C",
            "overflow-checks=off",
        ])
        .arg(fixture.0.join("src/main.rs"))
        .arg("--extern")
        .arg(format!("unicode_ident={}", named_library.display()))
        .arg("-o")
        .arg(&reference)
        .output()
        .unwrap();
    success(&output);
    let expected = Command::new(reference).output().unwrap();
    success(&expected);
    for args in [&[][..], &["-O"][..]] {
        let actual = fixture.run("run", args);
        success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
