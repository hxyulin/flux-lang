//! Old-edition opaque libraries retain their capture contract in modern consumers.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use flux_syntax::Edition;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
fn run(command: &mut Command, directory: &Path, label: &str) -> Output {
    support::process::run(command, directory, label, Duration::from_secs(60)).completed()
}
#[test]
fn portable_rpit_matches_rustc() {
    support::oracle::compare_fixture("pre2024_rpit");
}
#[test]
fn old_edition_libraries_match_rustc_off_and_on() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/pre2024-rpit")
        .join(std::process::id().to_string());
    std::fs::create_dir_all(&directory).unwrap();
    let library = directory.join("legacy.rs");
    let consumer = directory.join("consumer.rs");
    std::fs::write(
        &library,
        include_str!("fixtures/pre2024_rpit/library.rs.txt"),
    )
    .unwrap();
    std::fs::write(
        &consumer,
        include_str!("fixtures/pre2024_rpit/consumer.rs.txt"),
    )
    .unwrap();
    for (edition, text) in [(Edition::E2018, "2018"), (Edition::E2021, "2021")] {
        let rlib = directory.join(format!("liblegacy-{text}.rlib"));
        support::oracle::success(
            &run(
                Command::new("rustc")
                    .args([
                        "--crate-name=legacy",
                        "--crate-type=rlib",
                        "-Cpanic=abort",
                        "--edition",
                        text,
                    ])
                    .arg(&library)
                    .arg("-o")
                    .arg(&rlib),
                &directory,
                &format!("rustc-library-{text}"),
            ),
            text,
        );
        let reference = directory.join(format!("reference-{text}"));
        support::oracle::success(
            &run(
                Command::new("rustc")
                    .args(["--edition=2024", "-Cpanic=abort", "--extern"])
                    .arg(format!("legacy={}", rlib.display()))
                    .arg(&consumer)
                    .arg("-o")
                    .arg(&reference),
                &directory,
                &format!("rustc-consumer-{text}"),
            ),
            text,
        );
        let expected = run(
            &mut Command::new(reference),
            &directory,
            &format!("rustc-run-{text}"),
        );
        support::oracle::success(&expected, text);
        assert_eq!(expected.stdout, b"rpit 3 41 17 4\n");
        let mut db = SourceDb::new();
        let lib = db.load_with_edition(&library, edition).unwrap();
        let main = db.load_with_edition(&consumer, Edition::E2024).unwrap();
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                SourceCrate {
                    root: lib,
                    name: Symbol::intern("legacy"),
                    dependencies: vec![],
                },
                SourceCrate {
                    root: main,
                    name: Symbol::intern("consumer"),
                    dependencies: vec![(Symbol::intern("legacy"), 0)],
                },
            ],
        );
        assert!(db.syntax_errors().is_empty());
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        for optimized in [false, true] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            let tier = if optimized {
                flux_codegen::OptimizationTier::Full
            } else {
                flux_codegen::OptimizationTier::None
            };
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = directory.join(format!("flux-{text}-{optimized}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = run(
                &mut Command::new(executable),
                &directory,
                &format!("flux-run-{text}-{optimized}"),
            );
            assert_eq!(actual.status.code(), expected.status.code());
            assert_eq!(actual.stdout, expected.stdout);
            assert_eq!(actual.stderr, expected.stderr);
        }
    }
}
