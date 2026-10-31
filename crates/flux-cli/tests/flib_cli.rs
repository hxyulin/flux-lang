mod flib_support;

use std::process::Command;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

fn scratch() -> Scratch {
    Scratch::new(std::env::temp_dir().join("flux-flib-tests"))
}

#[test]
fn library_output_contains_checked_metadata_and_one_object() {
    let scratch = scratch();
    let source = scratch.path().join("a.rs");
    std::fs::write(&source,"pub fn double(x: u32) -> u32 { x * 2 } pub fn identity<T>(x: T) -> T { x } pub const fn add(x: u32) -> u32 { x + 1 }").unwrap();
    let output = scratch.path().join("a.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["build", "--crate-type", "lib", "--crate-name", "a"])
            .arg(&source)
            .arg("-o")
            .arg(&output),
        "compile",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let archive = flux_cli::flib::Archive::decode(&std::fs::read(&output).unwrap()).unwrap();
    assert!(archive.metadata.starts_with(b"FLUXMETA"));
    assert!(archive.object.as_ref().is_some_and(|o| !o.is_empty()));
    let metadata = scratch.path().join("check.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["build", "--crate-type", "lib", "--metadata-only"])
            .arg(&source)
            .arg("-o")
            .arg(&metadata),
        "compile",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let archive = flux_cli::flib::Archive::decode(&std::fs::read(&metadata).unwrap()).unwrap();
    assert!(archive.object.is_none());
    std::fs::write(&source, "pub fn identity<T>(x: T) -> T { x }").unwrap();
    let generic = scratch.path().join("generic.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["build", "--crate-type", "lib"])
            .arg(&source)
            .arg("-o")
            .arg(&generic),
        "generic-only",
    );
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let archive = flux_cli::flib::Archive::decode(&std::fs::read(&generic).unwrap()).unwrap();
    assert!(
        archive.object.is_some(),
        "generic-only library still needs an object-capable artifact"
    );
}

#[test]
fn erroneous_library_does_not_write_an_artifact() {
    let scratch = scratch();
    let source = scratch.path().join("bad.rs");
    std::fs::write(&source, "pub fn bad() -> u32 { true }").unwrap();
    let output = scratch.path().join("bad.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .args(["build", "--crate-type", "lib"])
            .arg(&source)
            .arg("-o")
            .arg(&output),
        "compile",
    );
    assert!(!result.status.success());
    assert!(!output.exists());
    assert!(String::from_utf8_lossy(&result.stderr).contains("mismatched types"));
}

#[test]
fn metadata_only_dependencies_support_checking_and_reject_native_linking() {
    let s = scratch();
    std::fs::write(
        s.path().join("a.rs"),
        "pub const N: usize = 2; pub fn twice(x: u32) -> u32 { x * 2 }",
    )
    .unwrap();
    std::fs::write(
        s.path().join("b.rs"),
        "pub fn value() -> u32 { a::twice(a::N as u32) }",
    )
    .unwrap();
    std::fs::write(
        s.path().join("main.rs"),
        "fn main() { println!(\"{}\", a::twice(21)); }",
    )
    .unwrap();
    for args in [
        vec![
            "build",
            "a.rs",
            "--crate-type",
            "lib",
            "--metadata-only",
            "-o",
            "a.flib",
        ],
        vec![
            "build",
            "b.rs",
            "--crate-type",
            "lib",
            "--metadata-only",
            "--extern",
            "a=a.flib",
            "-o",
            "b.flib",
        ],
    ] {
        let output = flib_support::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .current_dir(s.path())
                .args(args),
            "metadata-check",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if s.path().join("a.rs").exists() {
            std::fs::remove_file(s.path().join("a.rs")).unwrap();
        }
    }
    let output = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .current_dir(s.path())
            .args(["build", "main.rs", "--extern", "a=a.flib", "-o", "program"]),
        "metadata-link-rejection",
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("no native object"));
    assert!(!s.path().join("program").exists());
}
