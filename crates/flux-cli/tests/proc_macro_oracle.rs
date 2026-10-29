//! Compare identical macro sources through rustc's host and Flux's host.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(90)).completed()
}

struct Fixture(PathBuf);
impl Fixture {
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn checked(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn token_shapes_and_forwarded_expression_match_rustc() {
    let fixture =
        Fixture(std::env::temp_dir().join(format!("flux-proc-oracle-{}", std::process::id())));
    fixture.write("Cargo.toml", "[package]\nname='oracle-consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\noracle={path='macros'}\n");
    fixture.write(
        "macros/Cargo.toml",
        "[package]\nname='oracle'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    fixture.write(
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
use proc_macro::{TokenStream, TokenTree, Literal};
fn marker() -> bool { cfg!(proc_macro) }
#[proc_macro]
pub fn identity(input: TokenStream) -> TokenStream { input }
#[proc_macro]
pub fn shape(input: TokenStream) -> TokenStream {
    assert!(crate::marker());
    fn visit(input: TokenStream, out: &mut String) {
        for token in input {
            match token {
                TokenTree::Group(g) => {
                    out.push_str(&format!("G{:?}[", g.delimiter()));
                    visit(g.stream(), out);
                    out.push(']');
                }
                TokenTree::Ident(i) => out.push_str(&format!("I{};", i)),
                TokenTree::Punct(p) => out.push_str(&format!("P{}{:?};", p.as_char(), p.spacing())),
                TokenTree::Literal(l) => out.push_str(&format!("L{};", l)),
            }
        }
    }
    let mut output = String::new();
    visit(input, &mut output);
    TokenTree::Literal(Literal::string(&output)).into()
}
"#,
    );
    fixture.write(
        "src/main.rs",
        r##"
macro_rules! forwarded { ($value:expr) => { oracle::identity!($value) * 3 }; }
fn main() {
    println!("{}", oracle::shape!(&'a r#type::name -> [u8; 2] => -1 r#"raw"#));
    // A direct call lets the proc tokenizer observe documentation comments.
    let docs = oracle::shape!(/// a doc comment
        struct Documented;);
    println!("{}", docs);
    println!("{}", forwarded!(1 + 2));
    let r#type = 17;
    println!("{}", oracle::identity!(r#type));
}
"##,
    );
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let library = fixture.0.join("liboracle.dylib");
    checked(run(
        Command::new(&rustc)
            .current_dir(&fixture.0)
            .args([
                "--edition=2024",
                "--crate-type=proc-macro",
                "--crate-name=oracle",
                "macros/src/lib.rs",
                "-o",
            ])
            .arg(&library),
        &fixture.0,
        "rustc-macro",
    ));
    let reference = fixture.0.join("reference");
    checked(run(
        Command::new(&rustc)
            .current_dir(&fixture.0)
            .args(["--edition=2024", "src/main.rs", "--extern"])
            .arg(format!("oracle={}", library.display()))
            .arg("-o")
            .arg(&reference),
        &fixture.0,
        "rustc-reference",
    ));
    let expected = checked(run(
        &mut Command::new(&reference),
        &fixture.0,
        "reference-run",
    ));
    let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
    command
        .current_dir(&fixture.0)
        .args(["package", "run", "--manifest-path", "Cargo.toml"])
        .env("CARGO_HOME", fixture.0.join("cargo-home"))
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
    let actual = checked(run(&mut command, &fixture.0, "flux-run"));
    // Main's doc tokenizer uses an ordinary string literal for this fixture;
    // rustc uses a raw literal with the same value. Normalize only that spelling
    // in the comparison, retaining every other raw literal and token shape.
    let doc_spelling = |bytes| {
        String::from_utf8(bytes)
            .unwrap()
            .replace("Lr\" a doc comment\";", "L\" a doc comment\";")
    };
    assert_eq!(doc_spelling(actual.stdout), doc_spelling(expected.stdout));
}

#[test]
fn non_nfc_source_shadowing_is_rejected_before_codegen() {
    let fixture =
        Fixture(std::env::temp_dir().join(format!("flux-proc-nfc-oracle-{}", std::process::id())));
    fixture.write("Cargo.toml", "[package]\nname='nfc-consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\noracle={path='macros'}\n");
    fixture.write(
        "macros/Cargo.toml",
        "[package]\nname='oracle'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    fixture.write("macros/src/lib.rs", "extern crate proc_macro; use proc_macro::TokenStream; #[proc_macro] pub fn identity(input: TokenStream) -> TokenStream { input }");
    fixture.write(
        "src/main.rs",
        "fn main() { let é = 1; let e\u{301} = 2; println!(\"{}\", oracle::identity!(é)); }",
    );
    let library = fixture.0.join("liboracle.dylib");
    checked(run(
        Command::new("rustc")
            .current_dir(&fixture.0)
            .args([
                "--edition=2024",
                "--crate-type=proc-macro",
                "--crate-name=oracle",
                "macros/src/lib.rs",
                "-o",
            ])
            .arg(&library),
        &fixture.0,
        "rustc-macro",
    ));
    let reference = fixture.0.join("reference");
    checked(run(
        Command::new("rustc")
            .current_dir(&fixture.0)
            .args(["--edition=2024", "src/main.rs", "--extern"])
            .arg(format!("oracle={}", library.display()))
            .arg("-o")
            .arg(&reference),
        &fixture.0,
        "rustc-reference",
    ));
    assert_eq!(
        checked(run(
            &mut Command::new(reference),
            &fixture.0,
            "reference-run"
        ))
        .stdout,
        b"2\n"
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
    command
        .current_dir(&fixture.0)
        .args(["package", "build", "--manifest-path", "Cargo.toml"])
        .env("CARGO_HOME", fixture.0.join("cargo-home"))
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
    let output = run(&mut command, &fixture.0, "flux-build");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("non-nfc identifier spellings are not supported yet"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!fixture.0.join("target/flux/nfc-consumer").exists());
}
