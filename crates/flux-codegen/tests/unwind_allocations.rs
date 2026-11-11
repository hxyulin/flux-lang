//! Own-runtime unwinding releases allocator-visible storage exactly once.
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

const SOURCE: &str = include_str!("fixtures/unwind_allocations.rs.txt");

#[test]
fn own_runtime_releases_boxes_frames_payloads_and_headers() {
    let scratch = scratch::Scratch::new(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/unwind-allocations"),
    );
    let root = scratch.path();
    let input = root.join("input.rs");
    let c_source = root.join("probe.c");
    let c_object = root.join("probe.o");
    std::fs::write(&input, SOURCE).unwrap();
    std::fs::write(&c_source, include_str!("fixtures/unwind_allocations.c")).unwrap();
    let compile_c = process::run(
        Command::new("cc")
            .args(["-c", "-O2"])
            .arg(&c_source)
            .arg("-o")
            .arg(&c_object),
        root,
        "compile-probe",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        compile_c.status.success(),
        "{}",
        String::from_utf8_lossy(&compile_c.stderr)
    );
    let rust = root.join("rust");
    let build = process::run(
        Command::new("rustc")
            .args([
                "--edition=2024",
                "--cfg",
                "rust_control",
                "-C",
                "panic=unwind",
                "-C",
            ])
            .arg(format!("link-arg={}", c_object.display()))
            .arg(&input)
            .arg("-o")
            .arg(&rust),
        root,
        "rust-build",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let expected = process::run(
        &mut Command::new(&rust),
        root,
        "rust-run",
        Duration::from_secs(60),
    )
    .completed();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    assert_eq!(expected.stdout, b"allocation release passed\n");
    let (_, _, hir) = flux_hir::lower_text(&input, SOURCE);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file_with_panic_strategy(&hir, &typed, PanicStrategy::Unwind);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let rename = |name: &mut String| {
        if matches!(
            name.as_str(),
            "malloc" | "calloc" | "realloc" | "free" | "posix_memalign"
        ) {
            *name = format!("flux_{name}_probe");
        }
    };
    for declaration in &mut module.decls {
        rename(&mut declaration.name);
    }
    for function in &mut module.functions {
        let callees: Vec<_> = function.callees().map(|(id, _)| id).collect();
        for id in callees {
            rename(&mut function.callee_mut(id).name);
        }
    }
    let native = NativeLink::libraries(&[NativeLibrary {
        name: Symbol::intern("c++"),
        kind: NativeKind::Dylib,
    }]);
    let probe = std::fs::read(c_object).unwrap();
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
                    name: "allocator-probe",
                    bytes: &probe,
                },
            ],
            Target::Aarch64AppleDarwin,
            &binary,
            &native,
        )
        .unwrap();
        let actual = process::run(
            &mut Command::new(&binary),
            root,
            &format!("{name}-run"),
            Duration::from_secs(60),
        )
        .completed();
        assert_eq!(
            actual.status,
            expected.status,
            "{name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        let negative = process::run(
            Command::new(&binary).env("PROBE_OMIT_FREE", "1"),
            root,
            &format!("{name}-leak-control"),
            Duration::from_secs(60),
        )
        .completed();
        assert!(!negative.status.success());
        assert!(
            String::from_utf8_lossy(&negative.stderr).contains("outstanding"),
            "{}",
            String::from_utf8_lossy(&negative.stderr)
        );
    }
}
