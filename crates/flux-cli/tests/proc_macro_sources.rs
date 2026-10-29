//! Host source loading retains Rust's module, inclusion and cfg semantics.
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
            "flux-macro-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='macros'}\n");
        fixture.write(
            "macros/Cargo.toml",
            "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
        );
        fixture
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
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
fn nested_modules_paths_and_includes_preserve_source_location() {
    let fixture = Fixture::new();
    fixture.write(
        "macros/src/lib.rs",
        r#"
use proc_macro::TokenStream;
mod outer { pub mod inner; }
#[path = "../shared/helper.rs"] mod helper;
include!("included.rs");
const TEXT: &str = include_str!(concat!("../", "payload.txt"));
const BYTES: &[u8] = include_bytes!("../payload.txt");
const OUTSIDE: &str = include_str!("../../outside.txt");
#[proc_macro]
pub fn answer(_: TokenStream) -> TokenStream {
    assert!(file!().ends_with("macros/src/lib.rs"));
    assert_eq!(TEXT, "input");
    assert_eq!(OUTSIDE, "outside package");
    assert_eq!(BYTES.len(), 5);
    format!("{}u32", outer::inner::value() + helper::value() + included()).parse().unwrap()
}
"#,
    );
    fixture.write(
        "macros/src/outer/inner.rs",
        "pub fn value() -> u32 { include!(\"value.rs\") }",
    );
    fixture.write("macros/src/outer/value.rs", "10");
    fixture.write("macros/shared/helper.rs", "pub fn value() -> u32 { 20 }");
    fixture.write("macros/src/included.rs", "fn included() -> u32 { 12 }");
    fixture.write("macros/payload.txt", "input");
    fixture.write("outside.txt", "outside package");
    fixture.write(
        "src/main.rs",
        "fn main() { println!(\"{}\", macros::answer!()); }",
    );
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42\n");
}

#[test]
fn cfg_and_nested_cfg_attr_select_only_enabled_exports() {
    let fixture = Fixture::new();
    fixture.write(
        "macros/src/lib.rs",
        r#"
use proc_macro::TokenStream;
#[cfg(any())]
#[proc_macro]
pub fn selected(_: TokenStream) -> TokenStream { panic!("disabled export ran") }
#[cfg(all(proc_macro, unix, target_os = r"macos"))]
#[cfg_attr(proc_macro, cfg_attr(all(), proc_macro))]
pub fn selected(_: TokenStream) -> TokenStream { "42".parse().unwrap() }
#[cfg_attr(proc_macro, proc_macro_derive(Answer, attributes(answer_helper)))]
pub fn answer(_: TokenStream) -> TokenStream { "fn answer() -> u32 { 7 }".parse().unwrap() }
"#,
    );
    fixture.write("src/main.rs", "#[derive(macros::Answer)] #[answer_helper] struct S; fn main() { println!(\"{} {}\", macros::selected!(), answer()); }");
    let output = fixture.run();
    success(&output);
    assert_eq!(output.stdout, b"42 7\n");
}
