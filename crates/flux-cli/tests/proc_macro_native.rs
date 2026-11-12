//! Flux compiles the macro token runtime and host without invoking rustc.
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
            "flux-native-macro-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )));
        fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='macros'}\n");
        fixture.write(
            "macros/Cargo.toml",
            "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
        );
        fixture.write(
            "src/main.rs",
            "fn main() { println!(\"{}\", macros::answer!()); }",
        );
        fixture.write(
            "tools/rustc",
            "#!/bin/sh\nprintf 'rustc invoked\\n' >> \"$RUSTC_CALL_LOG\"\nexit 86\n",
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            fixture.0.join("tools/rustc"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        fixture
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.0.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    fn run(&self, native: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&self.0)
            .args([
                "package",
                "run",
                "--manifest-path",
                "Cargo.toml",
                "--macro-host-compiler",
                if native { "flux" } else { "rustc" },
            ])
            .env("CARGO_HOME", self.0.join("cargo-home"))
            .env("MACRO_COUNTER", self.0.join("macro-counter"))
            .env(
                "EXPECTED_MACRO_SOURCE",
                self.0.join("macros/src/lib.rs").canonicalize().unwrap(),
            )
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
        if native {
            let path = std::env::join_paths(
                std::iter::once(self.0.join("tools"))
                    .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
            )
            .unwrap();
            command
                .env("PATH", path)
                .env("RUSTC", self.0.join("tools/rustc"))
                .env("RUSTC_CALL_LOG", self.0.join("rustc-calls"));
        }
        let output = run_command(&mut command, &self.0);
        if native {
            assert!(!self.0.join("rustc-calls").exists(), "rustc was invoked");
        }
        output
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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
fn flux_compiles_token_runtime_and_all_macro_kinds_without_rustc() {
    let fixture = Fixture::new();
    fixture.write("macros/src/lib.rs", r#"
use proc_macro::{TokenStream, TokenTree, Ident, Span};
mod nested { pub mod child; }
const TWO: u32 = include!("two.rs");
#[cfg(any())] pub fn disabled_public_item() {}
#[cfg(any())] fn main() {}
#[proc_macro]
#[cfg(unix,)]
pub fn answer(_: TokenStream) -> TokenStream {
    assert!(cfg!(proc_macro));
    assert_eq!(file!(), std::env::var("EXPECTED_MACRO_SOURCE").unwrap());
    format!("{}u32", nested::child::value() + TWO).parse().unwrap()
}
#[cfg_attr(all(), proc_macro_attribute)]
pub fn retain(_: TokenStream, item: TokenStream) -> TokenStream { item }
#[proc_macro_derive(Value)]
pub fn value(_: TokenStream) -> TokenStream { "impl S { fn value() -> u32 { 7 } }".parse().unwrap() }
#[proc_macro]
pub fn unicode(_: TokenStream) -> TokenStream {
    TokenTree::Ident(Ident::new("e\u{301}", Span::call_site())).into()
}
"#);
    fixture.write("macros/src/nested/child.rs", "pub fn value() -> u32 { 40 }");
    fixture.write("macros/src/two.rs", "2");
    fixture.write("src/main.rs", "#[macros::retain] fn number()->u32 {macros::answer!()}\n#[derive(macros::Value)] struct S;\nfn main(){let é = 9; println!(\"{} {} {}\",number(),S::value(),macros::unicode!());}\n");
    let expected = success(fixture.run(false));
    let actual = success(fixture.run(true));
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stdout, b"42 7 9\n");
}

#[test]
fn native_hosts_reject_invalid_exports_and_unsupported_dependencies() {
    let fixture = Fixture::new();
    for (source, diagnostic) in [
        (
            "use proc_macro::TokenStream; #[proc_macro] fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "must be public",
        ),
        (
            "use proc_macro::TokenStream; pub fn ordinary() {} #[proc_macro] pub fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "non-macro items",
        ),
        (
            "use proc_macro::TokenStream; #[proc_macro] pub unsafe fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "qualified or generic",
        ),
        (
            "use proc_macro::TokenStream; #[proc_macro(bad)] pub fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "do not accept arguments",
        ),
        (
            "use proc_macro::TokenStream; #[proc_macro_derive(A)] #[proc_macro_derive(B)] pub fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "duplicate procedural macro entry",
        ),
        (
            "use proc_macro::TokenStream; #[proc_macro_derive(A, attributes(foo bar))] pub fn answer(_:TokenStream)->TokenStream { TokenStream::new() }",
            "comma-separated helper names",
        ),
    ] {
        fixture.write("macros/src/lib.rs", source);
        let output = fixture.run(true);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(diagnostic),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    fixture.write("macros/Cargo.toml", "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n[dependencies]\nhelper={path='../helper'}\n");
    let output = fixture.run(true);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("dependencies of flux-compiled macro hosts")
    );
}

#[test]
fn native_file_writes_create_truncate_and_report_errors() {
    let fixture = Fixture::new();
    fixture.write(
        "file_driver.rs",
        r#"
fn main() {
    let path = std::env::var("FILE_OUTPUT").unwrap();
    let bytes = [0u8, 255u8, 128u8, 10u8, 42u8];
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, "x").unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"x");
    std::fs::write(&path, []).unwrap();
    assert!(std::fs::read(&path).unwrap().is_empty());
    assert!(std::fs::write("/not/a/real/directory/file", bytes).is_err());
    assert!(std::fs::write("bad\0path", bytes).is_err());
    println!("file writes match");
}
"#,
    );
    for compiler in ["rustc", "flux"] {
        let executable = fixture.0.join(compiler);
        let mut command = Command::new(if compiler == "flux" {
            env!("CARGO_BIN_EXE_fluxc")
        } else {
            "rustc"
        });
        if compiler == "flux" {
            command.arg("build");
        } else {
            command.arg("--edition=2024");
        }
        success(run_command(
            command
                .arg(fixture.0.join("file_driver.rs"))
                .arg("-o")
                .arg(&executable),
            &fixture.0,
        ));
        let output = success(run_command(
            Command::new(executable).env("FILE_OUTPUT", fixture.0.join("file-output")),
            &fixture.0,
        ));
        assert_eq!(output.stdout, b"file writes match\n");
    }
}

#[test]
fn each_successful_invocation_runs_once_in_type_and_expression_positions() {
    let fixture = Fixture::new();
    fixture.write(
        "macros/src/lib.rs",
        r#"
use proc_macro::TokenStream;
#[proc_macro]
pub fn once(input: TokenStream) -> TokenStream {
    let path = std::env::var("MACRO_COUNTER").unwrap();
    let count = match std::fs::read_to_string(&path) {
        Ok(text) => text.parse::<u32>().unwrap(),
        Err(_) => 0,
    };
    std::fs::write(path, (count + 1).to_string()).unwrap();
    input
}
"#,
    );
    fixture.write("src/main.rs", "type Word = macros::once!(u32);\nfn value() -> macros::once!(Word) { macros::once!(42) }\nfn main() { println!(\"{}\", value()); }\n");
    for native in [false, true] {
        let output = success(fixture.run(native));
        assert_eq!(output.stdout, b"42\n");
        assert_eq!(
            std::fs::read_to_string(fixture.0.join("macro-counter")).unwrap(),
            "3",
            "native={native}"
        );
        std::fs::remove_file(fixture.0.join("macro-counter")).unwrap();
    }
}

#[test]
fn native_availability_and_literal_constructors() {
    let fixture = Fixture::new();
    fixture.write("macros/src/lib.rs", r#"
use proc_macro::{TokenStream,Literal,Span};
#[proc_macro] pub fn answer(_:TokenStream)->TokenStream {
    assert!(proc_macro::is_available());
    assert!(!std::thread::spawn(proc_macro::is_available).join().unwrap());
    assert!(Span::call_site().source_text().is_none());
    let c = std::ffi::CStr::from_bytes_with_nul(b"hello\xff\0").unwrap();
    format!("({}, {}, {})",Literal::byte_character(255),Literal::c_string(c),Literal::byte_string(b"a\0b")).parse().unwrap()
}
"#);
    fixture.write(
        "src/main.rs",
        "fn main(){let(a,b,c)=macros::answer!();println!(\"{a} {:?} {:?}\",b.to_bytes(),c);}",
    );
    let expected = success(fixture.run(false));
    let actual = success(fixture.run(true));
    assert_eq!(actual.stdout, expected.stdout);
}

#[test]
fn generated_anonymous_lifetimes_without_rustc() {
    let fixture = Fixture::new();
    fixture.write("macros/src/lib.rs", r#"
use proc_macro::TokenStream;
#[proc_macro]
pub fn answer(_: TokenStream) -> TokenStream {
    "{ struct Borrowed<'a>(&'a u32); impl Borrowed<'_> { fn value(&self) -> u32 { *self.0 } } Borrowed(&37).value() }".parse().unwrap()
}
"#);
    let reference = success(fixture.run(false));
    let native = success(fixture.run(true));
    assert_eq!(native.stdout, reference.stdout);
    assert_eq!(native.stdout, b"37\n");
}

#[test]
fn dependency_globs_import_derives_before_item_collection() {
    let fixture = Fixture::new();
    fixture.write("Cargo.toml", "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nlibrary={path='library'}\n");
    fixture.write("library/Cargo.toml", "[package]\nname='library'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='../macros'}\n");
    fixture.write(
        "library/src/lib.rs",
        "pub mod items{pub trait Value{fn value()->u32;}pub use macros::Value;}",
    );
    fixture.write("macros/src/lib.rs", "use proc_macro::TokenStream;#[proc_macro_derive(Value)]pub fn derive(_:TokenStream)->TokenStream{\"impl library::items::Value for S{fn value()->u32{7}}\".parse().unwrap()}");
    fixture.write(
        "src/main.rs",
        "use library::items::*;#[derive(Value)]struct S;fn main(){println!(\"{}\",S::value());}",
    );
    let expected = success(fixture.run(false));
    let actual = success(fixture.run(true));
    assert_eq!(actual.stdout, expected.stdout);
    assert_eq!(actual.stdout, b"7\n");
}
