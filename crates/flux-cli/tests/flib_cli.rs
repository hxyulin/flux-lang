mod flib_support;

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "flux-flib-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn library_output_contains_checked_metadata_and_one_object() {
    let scratch = Scratch::new();
    let source = scratch.0.join("a.rs");
    std::fs::write(&source,"pub fn double(x: u32) -> u32 { x * 2 } pub fn identity<T>(x: T) -> T { x } pub const fn add(x: u32) -> u32 { x + 1 }").unwrap();
    let output = scratch.0.join("a.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
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
    let metadata = scratch.0.join("check.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
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
    let generic = scratch.0.join("generic.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
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
    let scratch = Scratch::new();
    let source = scratch.0.join("bad.rs");
    std::fs::write(&source, "pub fn bad() -> u32 { true }").unwrap();
    let output = scratch.0.join("bad.flib");
    let result = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
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
