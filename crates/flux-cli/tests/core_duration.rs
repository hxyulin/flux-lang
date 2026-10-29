//! Canonical core/std duration identity and allocation-free debug formatting.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
pub use core::{fmt, ops, str};
#[path = "../../../library/core/src/time.rs"]
#[allow(
    dead_code,
    reason = "the production duration implementation is compared with host Rust"
)]
mod implementation;
use std::path::Path;
use std::process::Command;
use support::oracle::success;

#[test]
fn core_duration_matches_rustc_off_and_on() {
    support::oracle::compare_fixture("core_duration");
}

#[test]
fn duration_debug_matches_host_without_allocator_dependency() {
    let mut state = 0x12736549abcdu64;
    for index in 0..1000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let secs = match index % 4 {
            0 => 0,
            1 => u64::MAX,
            _ => state,
        };
        let nanos = (state >> 16) as u32 % 1_000_000_000;
        let actual = implementation::Duration::new(secs, nanos);
        let expected = std::time::Duration::new(secs, nanos);
        assert_eq!(format!("{actual:?}"), format!("{expected:?}"));
        for p in [0, 1, 2, 3, 6, 9, 10, 12, 32, 64] {
            for w in [0, 3, 12, 30, 90] {
                assert_eq!(format!("{actual:.p$?}"), format!("{expected:.p$?}"));
                assert_eq!(
                    format!("{actual:多>+w$.p$?}"),
                    format!("{expected:多>+w$.p$?}")
                );
                assert_eq!(
                    format!("{actual:🦀^w$.p$?}"),
                    format!("{expected:🦀^w$.p$?}")
                );
                assert_eq!(format!("{actual:0w$.p$?}"), format!("{expected:0w$.p$?}"));
            }
        }
    }
}

#[test]
fn no_std_dependency_uses_the_same_duration_as_std() {
    use flux_hir::{SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/core-duration/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let library = root.join("library.rs");
    let consumer = root.join("consumer.rs");
    std::fs::write(&library,r#"#![no_std]
pub const BASE:core::time::Duration=core::time::Duration::new(1,750_000_000);
pub fn add(value:core::time::Duration)->core::time::Duration{value+BASE}
pub fn emit<W:core::fmt::Write>(out:&mut W)->core::fmt::Result{core::write!(out,"{:多^18.12?}",BASE)}
"#).unwrap();
    std::fs::write(&consumer,r#"extern crate duration_identity;
fn accept_std(value:std::time::Duration)->core::time::Duration{value}
fn main(){let value:std::time::Duration=duration_identity::add(std::time::Duration::from_millis(250));let core:core::time::Duration=accept_std(value);assert_eq!(core,core::time::Duration::from_secs(2));let mut text=String::new();duration_identity::emit(&mut text).unwrap();println!("identity {:?} {}",core,text);}
"#).unwrap();
    let rlib = root.join("libduration_identity.rlib");
    success(
        &Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=rlib",
                "--crate-name=duration_identity",
                "-Cpanic=abort",
            ])
            .arg(&library)
            .arg("-o")
            .arg(&rlib)
            .output()
            .unwrap(),
        "no_std reference library",
    );
    let reference = root.join("reference");
    success(
        &Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort"])
            .arg(&consumer)
            .arg("--extern")
            .arg(format!("duration_identity={}", rlib.display()))
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap(),
        "identity reference",
    );
    let expected = Command::new(reference).output().unwrap();
    success(&expected, "identity reference execution");
    let mut db = SourceDb::new();
    let library = db.load_with_edition(&library, Edition::E2024).unwrap();
    let consumer = db.load_with_edition(&consumer, Edition::E2024).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("duration_identity"),
                dependencies: vec![],
            },
            SourceCrate {
                root: consumer,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("duration_identity"), 0)],
            },
        ],
    );
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    for (name, tier) in [
        ("off", flux_codegen::OptimizationTier::None),
        ("on", flux_codegen::OptimizationTier::Full),
    ] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(name);
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = Command::new(executable).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code(), "{name}");
        assert_eq!(actual.stdout, expected.stdout, "{name}");
        assert_eq!(actual.stderr, expected.stderr, "{name}");
    }
}

#[test]
fn duration_privacy_and_traits_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-duration-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "core_private_field",
            "fn main(){let d=core::time::Duration::ZERO;let _=d.secs;}",
            false,
        ),
        (
            "std_private_constructor",
            "fn main(){let _=std::time::Duration{secs:0,nanos:0};}",
            false,
        ),
        (
            "core_std_traits",
            "fn yes<T:Send+Sync+Copy+Ord+core::hash::Hash+Default+core::fmt::Debug>(){}fn main(){yes::<core::time::Duration>();yes::<std::time::Duration>();}",
            true,
        ),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
