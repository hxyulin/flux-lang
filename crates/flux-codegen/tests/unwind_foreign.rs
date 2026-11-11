//! C-unwind may pass a panic through C++, but C++ cannot consume it.
#![cfg(all(target_arch = "aarch64", target_os = "macos"))]
use flux_codegen::{NativeLink, ObjectInput, OptimizationTier, Target};
use flux_hir::{NativeKind, NativeLibrary};
use flux_intern::Symbol;
use flux_mir::PanicStrategy;
use std::process::Command;
use std::time::Duration;
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

fn events(stderr: &[u8]) -> Vec<&str> {
    std::str::from_utf8(stderr)
        .unwrap()
        .lines()
        .filter(|line| line.starts_with("event "))
        .collect()
}

#[test]
fn foreign_rethrow_and_exception_ownership_match_rustc() {
    let scratch = scratch::Scratch::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/unwind-foreign"),
    );
    let root = scratch.path();
    let input = root.join("input.rs");
    let cpp = root.join("foreign.cpp");
    let object = root.join("foreign.o");
    let source = include_str!("fixtures/unwind_foreign.rs.txt");
    std::fs::write(&input, source).unwrap();
    std::fs::write(&cpp, include_str!("fixtures/unwind_foreign.cpp")).unwrap();
    let output = process::run(
        Command::new("c++")
            .args(["-std=c++17", "-c", "-O1"])
            .arg(cpp)
            .arg("-o")
            .arg(&object),
        root,
        "cpp-build",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rust = root.join("rust");
    let output = process::run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=unwind", "-l", "c++", "-C"])
            .arg(format!("link-arg={}", object.display()))
            .arg(&input)
            .arg("-o")
            .arg(&rust),
        root,
        "rust-build",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<_> = ["rethrow", "swallow", "foreign"]
        .into_iter()
        .map(|case| {
            let output = process::run(
                Command::new(&rust).arg(case),
                root,
                &format!("rust-{case}"),
                Duration::from_secs(30),
            )
            .completed();
            assert_eq!(output.status.success(), case == "rethrow");
            (case, output)
        })
        .collect();
    let (_, _, hir) = flux_hir::lower_text(&input, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let foreign = std::fs::read(object).unwrap();
    let native = NativeLink::libraries(&[NativeLibrary {
        name: Symbol::intern("c++"),
        kind: NativeKind::Dylib,
    }]);
    for (name, tier) in [
        ("none", OptimizationTier::None),
        ("full", OptimizationTier::Full),
    ] {
        let object = flux_codegen::compile_object_with_panic_strategy(
            &mut module.clone(),
            Target::Aarch64AppleDarwin,
            tier,
            PanicStrategy::Unwind,
        )
        .unwrap();
        let binary = root.join(name);
        flux_codegen::link_objects(
            &[
                ObjectInput {
                    name: "program",
                    bytes: &object,
                },
                ObjectInput {
                    name: "foreign",
                    bytes: &foreign,
                },
            ],
            Target::Aarch64AppleDarwin,
            &binary,
            &native,
        )
        .unwrap();
        for (case, expected) in &expected {
            let output = process::run(
                Command::new(&binary).arg(case),
                root,
                &format!("{name}-{case}"),
                Duration::from_secs(30),
            )
            .completed();
            assert_eq!(
                output.status,
                expected.status,
                "{name} {case}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(output.stdout, expected.stdout, "{name} {case}");
            assert_eq!(
                events(&output.stderr),
                events(&expected.stderr),
                "{name} {case}"
            );
            if *case != "rethrow" {
                assert_eq!(output.stderr, expected.stderr, "{name} {case}");
            }
        }
    }
}
