//! IEEE floating-point language operations match Rust.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn float_math_matches_rustc_off_and_on() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-float-math-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in ["float_math"] {
        let source = root.join(format!("tests/run/{name}.rs"));
        let reference = dir.join("rustc");
        let output = Command::new("rustc")
            .args([
                "--edition=2024",
                "-C",
                "panic=abort",
                "-C",
                "overflow-checks=off",
            ])
            .arg(&source)
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .take(20)
                .collect::<Vec<_>>()
                .join("\n")
        );
        let expected = Command::new(&reference).output().unwrap();
        assert!(
            expected.status.success(),
            "{}",
            String::from_utf8_lossy(&expected.stderr)
        );
        for optimize in [false, true] {
            let executable = dir.join(if optimize { "flux-opt" } else { "flux" });
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command.arg("build").arg(&source).arg("-o").arg(&executable);
            if optimize {
                command.arg("-O");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
                    .lines()
                    .take(20)
                    .collect::<Vec<_>>()
                    .join("\n")
            );
            let actual = Command::new(&executable).output().unwrap();
            assert_eq!(actual.status.code(), expected.status.code());
            if actual.stdout != expected.stdout {
                let a = String::from_utf8_lossy(&actual.stdout);
                let e = String::from_utf8_lossy(&expected.stdout);
                let mismatch = a.lines().zip(e.lines()).position(|(a, e)| a != e);
                panic!(
                    "{name}, optimize={optimize}, first differing line {mismatch:?}: actual {:?}, expected {:?}; artifacts {}",
                    mismatch.and_then(|i| a.lines().nth(i)),
                    mismatch.and_then(|i| e.lines().nth(i)),
                    dir.display()
                );
            }
            assert_eq!(actual.stderr, expected.stderr);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn math_availability_and_const_rules_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-math-oracle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut cases=vec![
        ("no_std_minmax".to_owned(),"#![no_std]\npub const fn f(x:f64)->f64{x.min(2.0).max(-2.0)}".to_owned(),true),
        ("no_std_explicit_std".to_owned(),"#![no_std]\nextern crate std;pub fn f(x:f64)->f64{x.sqrt()+x.floor()+x.powi(-3)}".to_owned(),true),
        ("no_std_alias_std".to_owned(),"#![no_std]\nextern crate std as native;type Scalar=f32;pub fn f(x:Scalar)->Scalar{Scalar::ceil(x)+Scalar::mul_add(x,x,x)}".to_owned(),true),
        ("qualified_alias".to_owned(),"type Scalar=f64;pub fn f(x:Scalar)->Scalar{Scalar::floor(x)+f64::sqrt(x)}".to_owned(),true),
        ("no_std_function_pointer".to_owned(),"#![no_std]\npub fn f()->fn(f64)->f64{f64::floor}".to_owned(),false),
        ("no_std_type_alias".to_owned(),"#![no_std]\ntype Scalar=f64;pub fn f(x:Scalar)->Scalar{Scalar::floor(x)}".to_owned(),false),
        ("const_sqrt".to_owned(),"const X:f32=4.0f32.sqrt();".to_owned(),false),
        ("const_powi".to_owned(),"const X:f64=2.0f64.powi(3);".to_owned(),false),
        ("powi_argument".to_owned(),"pub fn f(x:f32)->f32{x.powi(3u32)}".to_owned(),false),
        ("fma_width".to_owned(),"pub fn f(x:f32)->f32{x.mul_add(1.0f64,2.0f32)}".to_owned(),false),
    ];
    for ty in ["f32", "f64"] {
        for method in [
            "mul_add",
            "sqrt",
            "ceil",
            "floor",
            "trunc",
            "round_ties_even",
            "powi",
        ] {
            let args = match method {
                "mul_add" => "x,x",
                "powi" => "2",
                _ => "",
            };
            cases.push((
                format!("no_std_{ty}_{method}"),
                format!("#![no_std]\npub fn f(x:{ty})->{ty}{{x.{method}({args})}}"),
                false,
            ));
            cases.push((
                format!("no_std_qualified_{ty}_{method}"),
                format!(
                    "#![no_std]\npub fn f(x:{ty})->{ty}{{{ty}::{method}(x{})}}",
                    if args.is_empty() {
                        String::new()
                    } else {
                        format!(",{args}")
                    }
                ),
                false,
            ));
        }
        for method in [
            "min",
            "max",
            "mul_add",
            "ceil",
            "floor",
            "trunc",
            "round_ties_even",
        ] {
            let args = match method {
                "min" | "max" => "2.0",
                "mul_add" => "2.0,-3.0",
                _ => "",
            };
            cases.push((
                format!("const_{ty}_{method}"),
                format!("const X:{ty}=1.5{ty}.{method}({args});"),
                true,
            ));
        }
    }
    for (name, code, accepts) in cases {
        if !case_filter::selected(&name) {
            continue;
        }
        let source = dir.join(format!("{name}.rs"));
        std::fs::write(&source, code).unwrap();
        let rustc = Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "-o",
            ])
            .arg(dir.join(format!("{name}.rmeta")))
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&source)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
        assert!(
            !String::from_utf8_lossy(&flux.stderr).contains("panicked"),
            "{name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn user_std_name_cannot_define_primitive_impls() {
    let dir = std::env::temp_dir().join(format!("flux-math-std-spoof-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("lib.rs");
    std::fs::write(&source, "impl f64{pub fn user_math(self)->f64{self}} ").unwrap();
    let mut db = flux_hir::SourceDb::new();
    let root = db.load(&source).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[flux_hir::SourceCrate {
            root,
            name: flux_intern::Symbol::intern("std"),
            dependencies: Vec::new(),
        }],
    );
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message == "cannot define inherent `impl` for primitive types"),
        "{:?}",
        hir.diagnostics
    );
    // A virtual path or a dependency alias named std is not the embedded root.
    let mut db = flux_hir::SourceDb::new();
    let root = db.add(
        "<std>/lib.rs",
        "impl f64{pub fn user_math(self)->f64{self}}",
    );
    let hir = flux_hir::lower_sources(
        &mut db,
        &[flux_hir::SourceCrate {
            root,
            name: flux_intern::Symbol::intern("std"),
            dependencies: Vec::new(),
        }],
    );
    assert!(
        hir.diagnostics
            .iter()
            .any(|d| d.message == "cannot define inherent `impl` for primitive types"),
        "{:?}",
        hir.diagnostics
    );
    let mut db = flux_hir::SourceDb::new();
    let fake = db.add("fake_std.rs", "#![no_std]");
    let user = db.add(
        "user.rs",
        "#![no_std] extern crate std;pub fn f(x:f64)->f64{x.sqrt()}",
    );
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            flux_hir::SourceCrate {
                root: fake,
                name: flux_intern::Symbol::intern("fake_std"),
                dependencies: Vec::new(),
            },
            flux_hir::SourceCrate {
                root: user,
                name: flux_intern::Symbol::intern("user"),
                dependencies: vec![(flux_intern::Symbol::intern("std"), 0)],
            },
        ],
    );
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(
        typed.diagnostics.iter().any(|d| d.message.contains("sqrt")),
        "{:?}",
        typed.diagnostics
    );
    std::fs::remove_dir_all(dir).unwrap();
}

/// The recorded Darwin rustc 1.99.1 probe uses FRINTX and follows the changed
/// environment at O0/O3. Test Flux fixed ties-even directly, separately from
/// the default-environment differential above; this is no general fenv claim.
#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn nearest_even_ignores_ambient_rounding_modes() {
    let dir = std::env::temp_dir().join(format!("flux-math-fenv-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("main.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/floats/rounding_mode.rs.txt"),
    )
    .unwrap();
    for optimize in [false, true] {
        let executable = dir.join(format!("flux-{optimize}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimize {
            command.arg("-O");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .take(20)
                .collect::<Vec<_>>()
                .join("\n")
        );
        let actual = Command::new(&executable).output().unwrap();
        assert!(
            actual.status.success(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
