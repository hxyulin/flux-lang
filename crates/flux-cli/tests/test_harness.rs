//! Native tests compiled from untouched Rust test attributes.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
fn run(root: &Path, label: &str, command: &mut Command) -> Output {
    support::process::run(command, root, label, Duration::from_secs(180)).completed()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn native_test_discovery_filters_and_failures() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/native-test-harness"),
    );
    let root = scratch.path();
    let source = r#"
fn main(){panic!("original main must not run");}
#[cfg(test)] mod private {
 #[test] fn ordinary(){println!("captured-parent");std::thread::spawn(||println!("captured-child")).join().unwrap();assert_eq!(2+2,4);}
 #[test] #[should_panic(expected="needle")] fn expected(){panic!("a needle");}
 #[test] #[ignore="negative control"] fn ignored(){panic!("failure");}
 #[test] fn result()->Result<(), &'static str>{Ok(())}
 macro_rules! generate{()=>{#[test] fn expanded(){}}} generate!();
 mod nested {#[test] fn child(){}}
}
mod external;
mod parallel {
 static B:std::sync::Barrier=std::sync::Barrier::new(2);
 #[test] #[ignore] fn a(){B.wait();}
 #[test] #[ignore] fn b(){B.wait();}
}
"#;
    std::fs::write(root.join("main.rs"), source).unwrap();
    std::fs::write(root.join("external.rs"), "#[test] fn loaded(){}\n").unwrap();
    let binary = root.join("tests");
    let build = run(
        root,
        "build",
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["test", "--no-run", "--fast-build"])
            .arg(root.join("main.rs"))
            .arg("-o")
            .arg(&binary),
    );
    success(&build);
    let list = run(root, "list", Command::new(&binary).arg("--list"));
    success(&list);
    let text = String::from_utf8_lossy(&list.stdout);
    assert!(text.contains("private::expanded: test"), "{text}");
    assert!(text.contains("private::nested::child: test"), "{text}");
    assert!(text.contains("external::loaded: test"), "{text}");
    assert!(text.contains("9 tests"), "{text}");
    let all = run(root, "all", &mut Command::new(&binary));
    success(&all);
    assert!(String::from_utf8_lossy(&all.stdout).contains("6 passed; 0 failed; 3 ignored"));
    assert!(!String::from_utf8_lossy(&all.stdout).contains("captured-parent"));
    assert!(
        all.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&all.stderr)
    );
    let show = run(
        root,
        "show",
        Command::new(&binary).args(["private::ordinary", "--exact", "--show-output"]),
    );
    success(&show);
    assert!(String::from_utf8_lossy(&show.stdout).contains("captured-child"));
    let parallel = run(
        root,
        "parallel",
        Command::new(&binary).args(["parallel::", "--include-ignored", "--test-threads", "2"]),
    );
    success(&parallel);
    let exact = run(
        root,
        "exact",
        Command::new(&binary).args(["private::ordinary", "--exact"]),
    );
    success(&exact);
    assert!(String::from_utf8_lossy(&exact.stdout).contains("1 passed; 0 failed"));
    let ignored = run(
        root,
        "ignored",
        Command::new(&binary).args(["private::ignored", "--exact", "--ignored"]),
    );
    assert_eq!(ignored.status.code(), Some(101));
    assert_eq!(
        std::fs::read_to_string(root.join("main.rs")).unwrap(),
        source
    );
    let no_std = root.join("no_std.rs");
    std::fs::write(
        &no_std,
        "#![no_std]\n#[test] fn adds(){assert_eq!(1+1,2);}\n",
    )
    .unwrap();
    let output = run(
        root,
        "no-std",
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("test")
            .arg(&no_std)
            .arg("--fast-build"),
    );
    success(&output);
    std::fs::write(&no_std, "#![no_std]\n#[test] fn bad()->bool{true}\n").unwrap();
    let output = run(
        root,
        "bad-return",
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("test")
            .arg(&no_std)
            .arg("--fast-build"),
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Termination"));
}
#[test]
fn package_unit_integration_and_documentation_tests() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/package-test-harness"),
    );
    let root = scratch.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tests")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\n[package]\nname='harness-probe'\nversion='0.1.0'\nedition='2024'\n[features]\ndefault=['doc-feature']\ndoc-feature=[]\n[target.'cfg(unix)'.dev-dependencies]\nhelper={path='helper'}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname='harness-probe'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("helper/src")).unwrap();
    std::fs::write(
        root.join("helper/Cargo.toml"),
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("helper/src/lib.rs"),
        "pub fn value()->u32{assert!(!cfg!(test));7}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.rs"),
        r#"
/// ```
/// # let x=2;
/// assert_eq!(harness_probe::sum(x,3),5);
/// assert_eq!(helper::value(),7);
/// assert!(cfg!(feature="doc-feature"));
/// assert_eq!(env!("CARGO_PKG_NAME"),"harness-probe");
/// assert!(std::fs::read_to_string("Cargo.toml").unwrap().contains("harness-probe"));
/// ```
/// ```should_panic
/// panic!("expected");
/// ```
/// ```no_run
/// panic!("must not execute");
/// ```
/// ```compile_fail
/// let x:u32="wrong";
/// ```
/// ```ignore
/// invalid_code();
/// ```
/// ```python
/// not rust
/// ```
pub fn sum(a:u32,b:u32)->u32{a+b}
#[cfg(test)] mod unit {#[test] fn works(){assert_eq!(super::sum(1,2),3);assert_eq!(helper::value(),7);}}
"#,
    )
    .unwrap();
    std::fs::write(root.join("src/example.md"), "```no_run\nloop {}\n```\n").unwrap();
    let extra = r####"
#[doc = r#"```rust
assert!(true);
```"#]
pub fn raw_doc(){}
#[doc = include_str!("example.md")]
pub fn included_doc(){}
/// ```
/// let x=2;
#[doc="assert_eq!(x,2);"]
/// ```
pub fn mixed_docs(){}
#[cfg(doctest)] mod documentation_only {
 /// ```
 /// assert!(!cfg!(doctest));
 /// ```
 pub fn f(){}
}
/// ```no_run
/// #![allow(unused)]
/// let _value=std::fs::read_to_string("not-run")?;
/// # Ok::<(),std::io::Error>(())
/// ```
pub fn fallible_doc(){}
/// ```edition2015
/// extern crate harness_probe;
/// fn main(){assert_eq!(harness_probe::sum(1,2),3);}
/// ```
pub fn explicit_entry(){}
#[cfg(any())]
/// ```
/// this deliberately invalid code must not be compiled
/// ```
pub fn disabled_code(){}
#[cfg(debug_assertions)]
/// ```
/// this example belongs only to debug-assertion builds
/// ```
pub fn debug_only_documentation(){}
"####;
    use std::io::Write;
    std::fs::OpenOptions::new()
        .append(true)
        .open(root.join("src/lib.rs"))
        .unwrap()
        .write_all(extra.as_bytes())
        .unwrap();
    std::fs::write(
        root.join("tests/integration.rs"),
        "#[test] fn external(){assert_eq!(harness_probe::sum(1,2),3);}\n",
    )
    .unwrap();
    let output = run(
        root,
        "package",
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args(["test", "--offline", "--fast-build", "--manifest-path"])
            .arg(root.join("Cargo.toml")),
    );
    success(&output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("test unit::works ... ok"), "{stdout}");
    assert!(stdout.contains("test external ... ok"), "{stdout}");
    assert!(
        stdout.contains("doctest result: 10 passed; 0 failed; 1 ignored"),
        "{stdout}"
    );
    let docs = run(
        root,
        "docs-only",
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args([
                "test",
                "--offline",
                "--fast-build",
                "--doc",
                "--manifest-path",
            ])
            .arg(root.join("Cargo.toml")),
    );
    success(&docs);
    assert!(!String::from_utf8_lossy(&docs.stdout).contains("test unit::works"));
    std::fs::write(
        root.join("src/lib.rs"),
        "pub fn sum(a:u32,b:u32)->u32{let _=helper::value();a+b}\n",
    )
    .unwrap();
    let invalid = run(
        root,
        "dev-leak",
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args([
                "test",
                "--offline",
                "--fast-build",
                "--test",
                "integration",
                "--manifest-path",
            ])
            .arg(root.join("Cargo.toml")),
    );
    assert!(
        !invalid.status.success(),
        "dev dependency leaked into the ordinary library"
    );
    std::fs::write(
        root.join("src/lib.rs"),
        "/// ```compile_fail\n/// invalid_example();\n/// ```\npub fn documented(){}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("helper/src/lib.rs"),
        "pub fn value()->u32{missing_name}\n",
    )
    .unwrap();
    let bad_dependency = run(
        root,
        "doc-dependency-error",
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args([
                "test",
                "--offline",
                "--fast-build",
                "--doc",
                "-j1",
                "--manifest-path",
            ])
            .arg(root.join("Cargo.toml")),
    );
    assert!(!bad_dependency.status.success());
    assert!(
        !String::from_utf8_lossy(&bad_dependency.stdout).contains("doctest result:"),
        "dependency failure was counted as a compile_fail result"
    );
    assert!(String::from_utf8_lossy(&bad_dependency.stderr).contains("missing_name"));
}
