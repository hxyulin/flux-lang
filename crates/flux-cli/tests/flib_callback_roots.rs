//! Private adapters passed to generic callbacks keep their native helpers.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod flib_support;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

use std::process::{Command, Output};

fn successful(command: &mut Command, label: &str) -> Output {
    let output = flib_support::run(command, label);
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn private_callback_adapters_retain_helpers_after_source_removal() {
    let scratch = scratch::Scratch::new("out/flib-callback-roots");
    let directory = scratch.path().canonicalize().unwrap();
    let traits = r#"
pub trait Access { fn key<S: Seed>(&mut self, seed: S) -> S::Value; }
pub trait Seed { type Value; fn read<R: Reader>(self, reader: R) -> Self::Value; }
pub trait Reader { fn value(self) -> u32; }
pub trait Visitor { fn visit<A: Access>(self, access: A) -> u32; }
"#;
    let adapters = r#"
struct Adapter;
struct Reader(u32);
fn secret() -> u32 { 73 }
impl Reader { fn new() -> Self { Reader(secret()) } }
impl traits::Reader for Reader { fn value(self) -> u32 { self.0 } }
impl traits::Access for Adapter {
    fn key<S: traits::Seed>(&mut self, seed: S) -> S::Value { seed.read(Reader::new()) }
}
pub fn dispatch<V: traits::Visitor>(visitor: V) -> u32 { visitor.visit(Adapter) }

// A different private implementation is not reachable through this callback.
// Its monomorphic helper must not enter the owning native object.
struct Unused;
fn dead_entry() -> u32 { 999 }
impl traits::Access for Unused {
    fn key<S: traits::Seed>(&mut self, seed: S) -> S::Value {
        dead_entry();
        seed.read(Reader::new())
    }
}
"#;
    let program = r#"
struct Seed;
impl traits::Seed for Seed {
    type Value = u32;
    fn read<R: traits::Reader>(self, reader: R) -> u32 { reader.value() }
}
struct Visitor;
impl traits::Visitor for Visitor {
    fn visit<A: traits::Access>(self, mut access: A) -> u32 { access.key(Seed) }
}
fn main() { println!("{}", adapters::dispatch(Visitor)); }
"#;
    for (name, text) in [
        ("traits", traits),
        ("adapters", adapters),
        ("main", program),
    ] {
        std::fs::write(directory.join(format!("{name}.rs")), text).unwrap();
    }
    for name in ["traits", "adapters"] {
        let mut command = Command::new("rustc");
        command
            .current_dir(&directory)
            .args(["--edition=2024", "--crate-type=lib"])
            .arg(format!("{name}.rs"))
            .arg("-o")
            .arg(format!("lib{name}.rlib"));
        if name == "adapters" {
            command.args(["--extern", "traits=libtraits.rlib"]);
        }
        successful(&mut command, "oracle-library");
    }
    successful(
        Command::new("rustc").current_dir(&directory).args([
            "--edition=2024",
            "main.rs",
            "--extern",
            "traits=libtraits.rlib",
            "--extern",
            "adapters=libadapters.rlib",
            "-L",
            "dependency=.",
            "-o",
            "oracle",
        ]),
        "oracle-program",
    );
    let expected = successful(&mut Command::new(directory.join("oracle")), "oracle-run");
    assert_eq!(expected.stdout, b"73\n");
    for flags in [None, Some("-O"), Some("--fast-build")] {
        for (name, text) in [("traits", traits), ("adapters", adapters)] {
            std::fs::write(directory.join(format!("{name}.rs")), text).unwrap();
            let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
            command
                .current_dir(&directory)
                .arg("build")
                .arg(format!("{name}.rs"))
                .args(["--crate-type", "lib", "-o"])
                .arg(format!("{name}.flib"));
            if name == "adapters" {
                command.args(["--extern", "traits=traits.flib"]);
            }
            if let Some(flag) = flags {
                command.arg(flag);
            }
            successful(&mut command, "flux-library");
            if name == "adapters" {
                let bytes = std::fs::read(directory.join("adapters.flib")).unwrap();
                let object = flux_cli::flib::Archive::decode(&bytes)
                    .unwrap()
                    .object
                    .unwrap();
                assert!(
                    !object
                        .windows(b"dead_entry".len())
                        .any(|name| name == b"dead_entry"),
                    "unrelated private callbacks must not retain their helpers"
                );
            }
            std::fs::remove_file(directory.join(format!("{name}.rs"))).unwrap();
        }
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.current_dir(&directory).args([
            "build",
            "main.rs",
            "--extern",
            "traits=traits.flib",
            "--extern",
            "adapters=adapters.flib",
            "-o",
            "program",
        ]);
        if let Some(flag) = flags {
            command.arg(flag);
        }
        successful(&mut command, "flux-program");
        let actual = successful(&mut Command::new(directory.join("program")), "flux-run");
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
