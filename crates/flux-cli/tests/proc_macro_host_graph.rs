//! Host dependency graphs can execute native macros while Flux expands target macros.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

mod support;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn run_command(command: &mut Command, root: &Path) -> Output {
    support::process::run(command, root, "process", Duration::from_secs(120)).completed()
}

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let fixture = Self(std::env::temp_dir().join(format!(
            "flux-host-graph-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nouter={path='outer'}\n");
        fixture.write("outer/Cargo.toml", "[package]\nname='outer'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\nhelper={path='../helper'}\n");
        fixture.write("outer/src/lib.rs", "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn answer(_: TokenStream) -> TokenStream { helper::answer().to_string().parse().unwrap() }");
        fixture.write(
            "src/main.rs",
            "fn main() { println!(\"{}\", outer::answer!()); }",
        );
        fixture
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    fn run(&self) -> Output {
        self.run_with_host("rustc")
    }
    fn run_with_host(&self, host: &str) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&self.0)
            .args(["package", "run", "--manifest-path", "Cargo.toml"])
            .args(["--macro-host-compiler", host])
            .env("CARGO_HOME", self.0.join("cargo-home"))
            .env_remove("RUSTC_WRAPPER")
            .env_remove("RUSTC_WORKSPACE_WRAPPER");
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
        run_command(&mut command, &self.0)
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
fn helper_derive_and_direct_host_macro_dependencies() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nouter={path='outer'}\ntarget_derive={path='derive',package='derive-package'}\n");
    fixture.write("helper/Cargo.toml", "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nderived={path='../derive',package='derive-package'}\n");
    fixture.write(
        "helper/src/lib.rs",
        "#[derive(derived::Answer)] struct Value; pub fn answer() -> u32 { Value::answer() }",
    );
    fixture.write("derive/Cargo.toml", "[package]\nname='derive-package'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='derive_exports'\nproc-macro=true\n[dependencies]\nleaf={path='../leaf'}\n");
    fixture.write("derive/src/lib.rs", "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro_derive(Answer)] pub fn answer(_: TokenStream) -> TokenStream { format!(\"impl Value {{ fn answer() -> u32 {{ {} }} }}\", leaf::number()).parse().unwrap() } #[proc_macro] pub fn extra(_: TokenStream) -> TokenStream { \"1u32\".parse().unwrap() }");
    fixture.write(
        "leaf/Cargo.toml",
        "[package]\nname='leaf'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write("leaf/src/lib.rs", "pub fn number() -> u32 { 41 }");
    fixture.write("outer/Cargo.toml", "[package]\nname='outer'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\nhelper={path='../helper'}\ndirect={path='../derive',package='derive-package'}\n");
    fixture.write("outer/src/lib.rs", "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn answer(_: TokenStream) -> TokenStream { (helper::answer() + direct::extra!()).to_string().parse().unwrap() }");
    fixture.write("src/main.rs", "#[derive(target_derive::Answer)] struct Value; fn main() { println!(\"{} {}\", outer::answer!(), Value::answer()); }");
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42 41\n");
}

#[test]
fn host_macro_cycles_report_an_error() {
    let fixture = Fixture::new();
    fixture.write("helper/Cargo.toml", "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nouter={path='../outer'}\n");
    fixture.write(
        "helper/src/lib.rs",
        "pub fn answer() -> u32 { outer::answer!() }",
    );
    let output = fixture.run();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("cyclic path dependencies"));
}

#[test]
#[ignore = "requires MUTANTS_SOURCE pointing to unchanged mutants 0.0.3 sources"]
fn unchanged_mutants_attribute_crate() {
    let fixture = Fixture::new();
    let source = PathBuf::from(std::env::var_os("MUTANTS_SOURCE").expect("set MUTANTS_SOURCE"));
    let manifest = std::fs::read_to_string(source.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("version = \"0.0.3\""));
    // Use the registry manifest and source unchanged. Cargo metadata remains offline.
    let path = source.canonicalize().unwrap();
    fixture.write("Cargo.toml", &format!("[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmutants={{path={:?}}}\n", path.to_str().unwrap()));
    fixture.write(
        "src/main.rs",
        "#[mutants::skip] fn answer() -> u32 { 42 } fn main() { println!(\"{}\", answer()); }",
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn published_default_new_derive_matches_rustc() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nreal={path='published',package='default_new_macros'}\n");
    fixture.write(
        "published/Cargo.toml",
        include_str!("fixtures/default_new_macros/Cargo.toml"),
    );
    fixture.write(
        "published/src/lib.rs",
        include_str!("fixtures/default_new_macros/src/lib.rs"),
    );
    fixture.write(
        "src/main.rs",
        r#"
#[derive(real::DefaultNew)]
struct Named { value: u32 }
impl Named { fn new() -> Self { Self { value: 37 } } }
#[derive(real::DefaultNew)]
struct Tuple(u32);
impl Tuple { fn new() -> Self { Self(5) } }
#[derive(real::DefaultNew)]
struct Unit;
impl Unit {
    fn new() -> Self { Unit }
    fn value(&self) -> u32 { 1 }
}
fn main() {
    println!("{} {} {}", Named::default().value, Tuple::default().0, Unit::default().value());
}
"#,
    );
    let library = fixture.0.join("libpublished.dylib");
    let native = run_command(
        Command::new("rustc")
            .current_dir(&fixture.0)
            .args([
                "--edition=2024",
                "--crate-name=default_new_macros",
                "--crate-type=proc-macro",
                "published/src/lib.rs",
                "-o",
            ])
            .arg(&library),
        &fixture.0,
    );
    success(&native);
    let reference = fixture.0.join("reference");
    let compiled = run_command(
        Command::new("rustc")
            .current_dir(&fixture.0)
            .args(["--edition=2024", "src/main.rs", "--extern"])
            .arg(format!("real={}", library.display()))
            .arg("-o")
            .arg(&reference),
        &fixture.0,
    );
    success(&compiled);
    let expected = run_command(&mut Command::new(&reference), &fixture.0);
    success(&expected);
    let actual = fixture.run();
    success(&actual);
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stdout, b"37 5 1\n");
    let flux_host = fixture.run_with_host("flux");
    success(&flux_host);
    assert_eq!(flux_host.stdout, expected.stdout);
}
