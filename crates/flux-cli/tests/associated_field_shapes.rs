#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::process::Command;

#[test]
fn associated_field_shapes_match_rustc() {
    support::oracle::compare_fixture("associated_field_shapes");
}

#[test]
fn imported_associated_field_shapes_match_rustc() {
    let scratch = support::scratch::Scratch::new(
        std::env::temp_dir().join("flux-imported-associated-fields"),
    );
    let directory = scratch.path();
    std::fs::write(
        directory.join("dep.rs"),
        r#"
pub trait Family { type Item; }
pub struct Provider;
impl Family for Provider { type Item = u64; }
pub type Item = <Provider as Family>::Item;
pub struct Record<'a> { pub item: Item, pub marker: &'a u8, pub flag: bool }
pub fn make<'a>(item: Item, marker: &'a u8) -> Record<'a> { Record { item, marker, flag: true } }
"#,
    )
    .unwrap();
    std::fs::write(
        directory.join("main.rs"),
        r#"
fn main() { let marker = 17; let record = dep::make(0x123456789abcdef0, &marker);
assert_eq!(record.item, 0x123456789abcdef0); assert!(record.flag);
println!("{} {} {}", record.item, record.flag, *record.marker); }
"#,
    )
    .unwrap();
    let run = |command: &mut Command, label: &str| {
        let output = support::process::run(
            command.current_dir(directory),
            directory,
            label,
            std::time::Duration::from_secs(90),
        )
        .completed();
        support::oracle::success(&output, label);
        output
    };
    run(
        Command::new("rustc").args([
            "--edition=2024",
            "--crate-name=dep",
            "--crate-type=rlib",
            "dep.rs",
            "-o",
            "libdep.rlib",
        ]),
        "associated-fields-rustc-library",
    );
    run(
        Command::new("rustc").args([
            "--edition=2024",
            "main.rs",
            "--extern",
            "dep=libdep.rlib",
            "-o",
            "oracle",
        ]),
        "associated-fields-rustc-consumer",
    );
    let expected = run(
        &mut Command::new(directory.join("oracle")),
        "associated-fields-rustc-run",
    );
    for optimized in [false, true] {
        let mut dependency = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        dependency.args([
            "build",
            "dep.rs",
            "--crate-name",
            "dep",
            "--crate-type",
            "lib",
            "-o",
            "dep.flib",
        ]);
        if optimized {
            dependency.arg("-O");
        }
        run(&mut dependency, "associated-fields-flux-library");
        let mut consumer = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        consumer.args([
            "build",
            "main.rs",
            "--extern",
            "dep=dep.flib",
            "-o",
            "actual",
        ]);
        if optimized {
            consumer.arg("-O");
        }
        run(&mut consumer, "associated-fields-flux-consumer");
        let actual = run(
            &mut Command::new(directory.join("actual")),
            "associated-fields-flux-run",
        );
        assert_eq!(actual.stdout, expected.stdout, "optimized={optimized}");
        assert_eq!(actual.status.code(), expected.status.code());
    }
}
