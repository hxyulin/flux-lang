//! The normal driver consumes checked dependencies after their sources are removed.
mod flib_support;

use std::process::Command;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

fn scratch() -> Scratch {
    Scratch::new(std::env::temp_dir().join("flux-flib-tests"))
}
impl Scratch {
    fn command(&self, args: &[&str]) {
        let output = flib_support::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .current_dir(self.path())
                .args(args),
            "driver",
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn decoded_dependency_preserves_roots_constants_types_and_generic_mir() {
    let s = scratch();
    std::fs::write(
        s.path().join("a.rs"),
        r#"
pub struct Token(pub u32);
pub const ID: std::any::TypeId = std::any::TypeId::of::<Token>();
pub const N: usize = 3;
pub const P: &fn(u32) -> u32 = &(u32::count_ones as fn(u32) -> u32);
pub static S: fn(u32) -> u32 = u32::count_ones;
pub trait Counter { fn count(&self, n: u32) -> u32; }
impl Counter for Token { fn count(&self, n: u32) -> u32 { n.count_ones() + self.0 } }
pub const V: &dyn Counter = &Token(5);
pub fn identity<T>(x: T) -> T { x }
pub const fn next(x: usize) -> usize { x + 1 }
"#,
    )
    .unwrap();
    std::fs::write(s.path().join("b.rs"), r#"
fn main() {
    assert_eq!(a::ID, std::any::TypeId::of::<a::Token>());
    let value: &dyn std::any::Any = &a::Token(9);
    assert_eq!(value.downcast_ref::<a::Token>().unwrap().0, 9);
    let array: [u8; a::next(a::N)] = [1, 2, 3, 4];
    println!("{} {} {} {} {}", (a::P)(7), (a::S)(15), a::V.count(3), a::identity(42u32), array.len());
}
"#).unwrap();
    // rustc is the behavior oracle, and its private artifact stays outside the Flux path.
    let oracle = flib_support::run(
        Command::new("rustc").current_dir(s.path()).args([
            "--edition=2024",
            "--crate-type=lib",
            "a.rs",
        ]),
        "oracle-library",
    );
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let oracle = flib_support::run(
        Command::new("rustc").current_dir(s.path()).args([
            "--edition=2024",
            "b.rs",
            "--extern",
            "a=liba.rlib",
            "-o",
            "oracle",
        ]),
        "oracle-program",
    );
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let expected = flib_support::run(&mut Command::new(s.path().join("oracle")), "oracle-run");
    s.command(&["build", "a.rs", "--crate-type", "lib", "-o", "a.flib"]);
    std::fs::remove_file(s.path().join("a.rs")).unwrap();
    for optimized in [false, true] {
        let mut args = vec!["build", "b.rs", "--extern", "a=a.flib", "-o", "program"];
        if optimized {
            args.push("-O");
        }
        s.command(&args);
        let actual = flib_support::run(&mut Command::new(s.path().join("program")), "flux-run");
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
    }
}
