//! Projected generic calls retain private library implementation roots.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

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
fn projected_iterator_item_reaches_private_impl_and_helper_after_source_removal() {
    let scratch = scratch::Scratch::new("out/flib-projection-roots");
    let directory = scratch.path().canonicalize().unwrap();
    let library = r#"
trait Hidden { fn private(self) -> u32; }
fn secret() -> u32 { 73 }
impl Hidden for char {
    fn private(self) -> u32 { secret() + self as u32 }
}
struct PrivateIterator { done: bool }
fn iterator_secret() -> char { 'A' }
impl Iterator for PrivateIterator {
    type Item = char;
    fn next(&mut self) -> Option<char> {
        if self.done { None } else { self.done = true; Some(iterator_secret()) }
    }
}
pub fn characters() -> impl Iterator<Item = char> { PrivateIterator { done: false } }
pub fn projected<I: Iterator<Item = char>>(mut input: I) -> u32 {
    <I::Item as Hidden>::private(input.next().unwrap())
}
"#;
    std::fs::write(directory.join("a.rs"), library).unwrap();
    std::fs::write(
        directory.join("b.rs"),
        r#"
fn main() {
    println!("{} {} {}", a::projected("flux".chars()), a::projected(['Z', 'Q'].into_iter()), a::projected(a::characters()));
}
"#,
    )
    .unwrap();
    successful(
        Command::new("rustc").current_dir(&directory).args([
            "--edition=2024",
            "--crate-type=lib",
            "a.rs",
            "-o",
            "liboracle_library.rlib",
        ]),
        "oracle-library",
    );
    successful(
        Command::new("rustc").current_dir(&directory).args([
            "--edition=2024",
            "b.rs",
            "--extern",
            "a=liboracle_library.rlib",
            "-o",
            "oracle",
        ]),
        "oracle-program",
    );
    let expected = successful(&mut Command::new(directory.join("oracle")), "oracle-run");
    assert_eq!(expected.stdout, b"175 163 138\n");
    for optimized in [false, true] {
        // Build each library independently so both object emission modes retain the roots.
        std::fs::write(directory.join("a.rs"), library).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.current_dir(&directory).args([
            "build",
            "a.rs",
            "--crate-type",
            "lib",
            "-o",
            "a.flib",
        ]);
        if optimized {
            command.arg("-O");
        }
        successful(&mut command, "flux-library");
        std::fs::remove_file(directory.join("a.rs")).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(&directory)
            .args(["build", "b.rs", "--extern", "a=a.flib", "-o", "program"]);
        if optimized {
            command.arg("-O");
        }
        successful(&mut command, "flux-program");
        let actual = successful(&mut Command::new(directory.join("program")), "flux-run");
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert!(!directory.join("a.rs").exists());
    }
}
