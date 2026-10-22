//! Observable external use of non-exhaustive types, in both codegen modes.
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

fn success(output: Output) -> Output {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}

#[test]
fn non_exhaustive_dependency_runtime() {
    if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        return;
    }
    let root = std::env::temp_dir().join(format!(
        "flux-non-exhaustive-runtime-{}",
        std::process::id()
    ));
    write(
        &root,
        "Cargo.toml",
        "[package]\nname='consumer'\nversion='0.1.0'\nedition='2024'\n[dependencies]\ndep={path='dep'}\n",
    );
    write(
        &root,
        "dep/Cargo.toml",
        "[package]\nname='dep'\nversion='0.1.0'\nedition='2024'\n",
    );
    write(
        &root,
        "dep/src/lib.rs",
        r#"
#[non_exhaustive] pub struct Named { pub x: i32 }
#[non_exhaustive] pub struct Tuple(pub i32);
#[non_exhaustive] pub struct Unit;
#[non_exhaustive] pub enum Choice { A, B(i32), C { x: i32 } }
pub use Named as Renamed;
pub type Alias = Named;
pub fn named(x: i32) -> Named { Named { x } }
pub fn tuple(x: i32) -> Tuple { Tuple(x) }
pub fn unit() -> Unit { Unit }
pub fn choice(x: i32) -> Choice { if x == 0 { Choice::A } else if x == 1 { Choice::B(7) } else { Choice::C { x } } }
"#,
    );
    write(
        &root,
        "src/main.rs",
        r#"
fn inspect(x: i32) -> i32 {
    let dep::Renamed { x: a, .. } = dep::named(x);
    let b: dep::Alias = dep::named(x + 1);
    let dep::Alias { x: b, .. } = b;
    let dep::Tuple { 0: c, .. } = dep::tuple(x + 2);
    let dep::Unit { .. } = dep::unit();
    a + b + c + match dep::choice(x) { dep::Choice::A => 10, dep::Choice::B(v) => v, dep::Choice::C { x } => x, _ => 99 }
}
fn main() { println!("{} {} {}", inspect(0), inspect(1), inspect(2)); }
"#,
    );
    success(
        Command::new("rustc")
            .current_dir(&root)
            .args([
                "--edition=2024",
                "--crate-type=rlib",
                "--crate-name=dep",
                "dep/src/lib.rs",
            ])
            .output()
            .unwrap(),
    );
    success(
        Command::new("rustc")
            .current_dir(&root)
            .args([
                "--edition=2024",
                "--extern",
                "dep=libdep.rlib",
                "src/main.rs",
                "-o",
                "reference",
            ])
            .output()
            .unwrap(),
    );
    let expected = success(Command::new(root.join("reference")).output().unwrap());
    for args in [&[][..], &["-O"][..]] {
        let actual = success(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .current_dir(&root)
                .args(["package", "run", "--manifest-path", "Cargo.toml"])
                .args(args)
                .output()
                .unwrap(),
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    fs::remove_dir_all(root).unwrap();
}
