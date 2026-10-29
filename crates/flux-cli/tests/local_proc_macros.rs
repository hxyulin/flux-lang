//! Local macro hosts bootstrap with rustc; the consuming program uses Flux.
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
        let root = std::env::temp_dir().join(format!(
            "flux-proc-macro-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Self(root);
        fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nrenamed={path='macros',package='macros'}\n");
        fixture.write(
            "macros/Cargo.toml",
            "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
        );
        fixture.write(
            "macros/src/lib.rs",
            r#"
extern crate proc_macro; use proc_macro::TokenStream;
#[proc_macro]
pub fn answer(_: TokenStream) -> TokenStream { "42u32".parse().unwrap() }
#[proc_macro]
pub fn passthrough(input: TokenStream) -> TokenStream { input }
#[proc_macro_attribute]
pub fn retain(attr: TokenStream, item: TokenStream) -> TokenStream {
    assert_eq!(attr.to_string(), "tag");
    item
}
#[proc_macro_derive(Value, attributes(value_helper))]
pub fn value(_: TokenStream) -> TokenStream {
    "impl S { fn derived() -> u32 { 7 } }".parse().unwrap()
}
"#,
        );
        fixture
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn run(&self) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&self.0)
            .args(["package", "run", "--manifest-path", "Cargo.toml"])
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
fn function_attribute_derive_imports_and_nested_expansion() {
    let fixture = Fixture::new();
    fixture.write(
        "src/main.rs",
        r#"
use renamed::{answer as number, passthrough, retain, Value};
#[retain(tag)]
fn generated() -> u32 { passthrough!(number!()) }
#[derive(Value)]
#[value_helper]
struct S;
fn main() { println!("{} {}", generated(), S::derived()); }
"#,
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42 7\n");
    assert!(!fixture.0.join("target/debug").exists());
}

#[test]
fn proc_macro_dependency_of_target_library() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nordinary={path='ordinary'}\n");
    fixture.write("ordinary/Cargo.toml", "[package]\nname='ordinary'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nrenamed={path='../macros',package='macros'}\n");
    fixture.write(
        "ordinary/src/lib.rs",
        "pub fn generated() -> u32 { renamed::answer!() }",
    );
    fixture.write(
        "src/main.rs",
        "fn main() { println!(\"{}\", ordinary::generated()); }",
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn macro_host_uses_transitive_local_helpers() {
    let fixture = Fixture::new();
    fixture.write("macros/Cargo.toml", "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\nhost_helper={path='../helper',package='helper'}\n");
    fixture.write("helper/Cargo.toml", "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n[lib]\nname='custom_helper'\n[dependencies]\nleaf={path='../leaf'}\n");
    fixture.write("helper/src/lib.rs", "pub fn output() -> &'static str { assert!(env!(\"CARGO_MANIFEST_DIR\").ends_with(\"helper\")); leaf::output() }");
    fixture.write(
        "leaf/Cargo.toml",
        "[package]\nname='leaf'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write(
        "leaf/src/lib.rs",
        "pub fn output() -> &'static str { \"23u32\" }",
    );
    fixture.write("macros/src/lib.rs", "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn answer(_: TokenStream) -> TokenStream { assert!(env!(\"CARGO_MANIFEST_DIR\").ends_with(\"macros\")); host_helper::output().parse().unwrap() }");
    fixture.write(
        "src/main.rs",
        "fn main() { println!(\"{}\", renamed::answer!()); }",
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"23\n");
}

#[test]
fn macro_panic_and_invalid_output_report_errors() {
    for body in ["panic!(\"macro failed\")", "\"fn\".parse().unwrap()"] {
        let fixture = Fixture::new();
        fixture.write("macros/src/lib.rs", &format!("extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn fail(_: TokenStream) -> TokenStream {{ {body} }}"));
        fixture.write("src/main.rs", "fn main() { let _ = renamed::fail!(); }");
        let output = fixture.run();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("error"), "{stderr}");
        if body.starts_with("panic") {
            assert!(stderr.contains("macro failed"), "{stderr}");
        }
        assert!(
            !stderr.contains("cannot validate procedural macro crate"),
            "{stderr}"
        );
        assert!(!fixture.0.join("target/flux/consumer").exists());
    }
}

#[test]
fn wrong_macro_kind_reports_invocation_error() {
    for (source, expected) in [
        (
            "fn main() { renamed::Value!(); }",
            "expected a function-like macro",
        ),
        (
            "#[renamed::answer] fn f() {} fn main() {}",
            "attributes like this are not supported yet",
        ),
        (
            "#[derive(renamed::retain)] struct S; fn main() {}",
            "expected a derive macro",
        ),
    ] {
        let fixture = Fixture::new();
        fixture.write("src/main.rs", source);
        let output = fixture.run();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains(expected), "{stderr}");
        assert!(
            !stderr.contains("cannot validate procedural macro crate"),
            "{stderr}"
        );
    }
}

#[test]
fn cfg_disabled_macro_item_is_not_expanded_and_attributes_chain() {
    let fixture = Fixture::new();
    fixture.write(
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
use proc_macro::TokenStream;
#[proc_macro_attribute]
pub fn bomb(_: TokenStream, _: TokenStream) -> TokenStream { panic!("disabled macro invoked") }
#[proc_macro_attribute]
pub fn retain(attr: TokenStream, item: TokenStream) -> TokenStream {
    assert_eq!(attr.to_string(), "tag");
    item
}
"#,
    );
    fixture.write(
        "src/main.rs",
        r#"
#[cfg(any())]
#[renamed::bomb]
fn disabled() {}
#[renamed::retain(tag)]
#[renamed::retain(tag)]
fn retained() -> u32 { 17 }
fn main() { println!("{}", retained()); }
"#,
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"17\n");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("disabled macro invoked"));
}
