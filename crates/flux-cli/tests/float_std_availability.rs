//! Std float methods follow loaded crate metadata, not import spelling.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use flux_hir::{SourceCrate, SourceDb};
use flux_intern::Symbol;
use std::path::Path;
use std::process::Command;

fn source(root: flux_hir::FileId, name: &str, dependencies: &[(&str, usize)]) -> SourceCrate {
    SourceCrate {
        root,
        name: Symbol::intern(name),
        dependencies: dependencies
            .iter()
            .map(|&(n, i)| (Symbol::intern(n), i))
            .collect(),
    }
}

fn rust_library(
    dir: &Path,
    name: &str,
    code: &str,
    dependency: Option<(&str, &Path)>,
) -> std::path::PathBuf {
    let path = dir.join(format!("{name}.rs"));
    std::fs::write(&path, code).unwrap();
    let library = dir.join(format!("lib{name}.rlib"));
    let mut command = Command::new("rustc");
    command
        .args([
            "--edition=2024",
            "--crate-type=rlib",
            "-C",
            "panic=abort",
            "--crate-name",
            name,
        ])
        .arg(&path)
        .arg("-o")
        .arg(&library);
    if let Some((name, path)) = dependency {
        command
            .arg("--extern")
            .arg(format!("{name}={}", path.display()))
            .arg("-L")
            .arg(format!("dependency={}", dir.display()));
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    library
}

#[test]
fn loaded_std_graph_matches_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-float-std-graph-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dep_code = "pub struct Marker;pub fn value()->u32{7}";
    let bare_code = "#![no_std] pub fn value()->u32{7}";
    let bridge_code = "#![no_std] extern crate dep;pub fn value()->u32{dep::value()}";
    let dep = rust_library(&dir, "dep", dep_code, None);
    let bare = rust_library(&dir, "bare", bare_code, None);
    let bridge = rust_library(&dir, "bridge", bridge_code, Some(("dep", &dep)));
    // The fake dependency is deliberately no_std. Its name grants no std identity.
    let fake = rust_library(&dir, "fake", bare_code, None);
    for (name, decl, body, dependency, accepts) in [
        ("unused", "", "", "dep", false),
        ("old_unused", "", "", "dep", false),
        ("old_extern", "extern crate dep;", "", "dep", true),
        (
            "old_nested",
            "mod n{extern crate std as platform;}",
            "",
            "dep",
            true,
        ),
        (
            "old_shadowed_import",
            "mod dep{pub fn value()->u32{0}}use dep::value;",
            "let _=value();",
            "dep",
            false,
        ),
        (
            "generic_shadow",
            "trait Mark{type Marker;}pub fn g<dep:Mark>(x:f64)->f64{let _:Option<dep::Marker>=None;x.sqrt()}",
            "",
            "dep",
            false,
        ),
        ("extern", "extern crate dep;", "", "dep", true),
        ("nested_extern", "mod n{extern crate dep;}", "", "dep", true),
        (
            "nested_renamed",
            "mod n{extern crate std as platform;pub fn f(x:f64)->f64{x.sqrt()}}",
            "",
            "dep",
            true,
        ),
        (
            "nested_std",
            "mod n{extern crate std;pub fn f(x:f64)->f64{x.floor()}}",
            "",
            "dep",
            true,
        ),
        (
            "renamed_dep",
            "extern crate dep as platform;",
            "",
            "dep",
            true,
        ),
        ("import", "use dep as platform;", "", "dep", true),
        ("value_path", "", "let _=dep::value();", "dep", true),
        (
            "late_type_path",
            "",
            "let _:Option<dep::Marker>=None;",
            "dep",
            true,
        ),
        ("transitive", "extern crate bridge;", "", "bridge", true),
        ("unused_transitive", "", "", "bridge", false),
        ("bare_extern", "extern crate bare;", "", "bare", false),
        ("fake_std", "extern crate std;", "", "std", false),
        (
            "shadowed",
            "mod dep{pub fn value()->u32{0}}",
            "let _=dep::value();",
            "dep",
            false,
        ),
        (
            "shadowed_import",
            "mod dep{pub fn value()->u32{0}}use dep::value;",
            "let _=value();",
            "dep",
            true,
        ),
        (
            "explicit_local_import",
            "mod dep{pub fn value()->u32{0}}use self::dep::value;",
            "let _=value();",
            "dep",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let code = format!(
            "#![no_std]\n{decl}\npub fn f(x:f64)->f64{{let result=x.sqrt()+x.floor();{body}result}}"
        );
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &code).unwrap();
        let library = match dependency {
            "bridge" => &bridge,
            "bare" => &bare,
            "std" => &fake,
            _ => &dep,
        };
        let edition = if name.starts_with("old_") {
            "2015"
        } else {
            "2024"
        };
        let rustc = Command::new("rustc")
            .arg(format!("--edition={edition}"))
            .args(["--crate-type=lib", "--emit=metadata", "-C", "panic=abort"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("{dependency}={}", library.display()))
            .arg("-L")
            .arg(format!("dependency={}", dir.display()))
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let mut db = SourceDb::new();
        let d = db.add("dep.rs", dep_code);
        let b = db.add("bare.rs", bare_code);
        let bridge_root = db.add("bridge.rs", bridge_code);
        let fake_root = db.add("fake.rs", bare_code);
        let user = db
            .load_with_edition(
                &path,
                if edition == "2015" {
                    flux_syntax::Edition::E2015
                } else {
                    flux_syntax::Edition::E2024
                },
            )
            .unwrap();
        let target = match dependency {
            "bridge" => 2,
            "bare" => 1,
            "std" => 3,
            _ => 0,
        };
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                source(d, "dep", &[]),
                source(b, "bare", &[]),
                source(bridge_root, "bridge", &[("dep", 0)]),
                source(fake_root, "fake", &[]),
                source(user, "user", &[(dependency, target)]),
            ],
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert_eq!(
            typed.diagnostics.is_empty(),
            accepts,
            "Flux {name}: {:?}",
            typed.diagnostics
        );
        if accepts {
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(mir.diagnostics.is_empty(), "{name}: {:?}", mir.diagnostics);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn nested_and_transitive_std_native_off_and_on() {
    let dir = std::env::temp_dir().join(format!("flux-float-std-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dep_code = "pub fn value()->u32{7}";
    let bridge_code = "#![no_std] extern crate dep;pub fn value()->u32{dep::value()}";
    let dep = rust_library(&dir, "dep", dep_code, None);
    let bridge = rust_library(&dir, "bridge", bridge_code, Some(("dep", &dep)));
    for (name, imports) in [
        (
            "nested",
            "mod n{extern crate std as platform;pub fn f(x:f64)->f64{x.sqrt()}}",
        ),
        ("transitive", "extern crate bridge;"),
    ] {
        let code = format!(
            "#![no_std]\n{imports}\nfn main(){{let x=std_value();assert_eq!(x.to_bits(),7.0f64.to_bits());let f:fn(f32)->f32=f32::sqrt;assert_eq!(f(9.0).to_bits(),3.0f32.to_bits());}}fn std_value()->f64{{4.0f64.sqrt()+5.5f64.floor()}}"
        );
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &code).unwrap();
        let reference = dir.join(format!("rustc-{name}"));
        let output = Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&path)
            .arg("--extern")
            .arg(format!("bridge={}", bridge.display()))
            .arg("-L")
            .arg(format!("dependency={}", dir.display()))
            .arg("-o")
            .arg(&reference)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected = Command::new(reference).output().unwrap();
        assert!(expected.status.success());
        let mut db = SourceDb::new();
        let d = db.add("dep.rs", dep_code);
        let b = db.add("bridge.rs", bridge_code);
        let u = db.add("user.rs", &code);
        let hir = flux_hir::lower_sources(
            &mut db,
            &[
                source(d, "dep", &[]),
                source(b, "bridge", &[("dep", 0)]),
                source(u, "user", &[("bridge", 1)]),
            ],
        );
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        for (label, tier) in [
            ("off", flux_codegen::OptimizationTier::None),
            ("on", flux_codegen::OptimizationTier::Full),
        ] {
            let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
            let target = flux_codegen::Target::Aarch64AppleDarwin;
            let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
            let executable = dir.join(format!("flux-{name}-{label}"));
            flux_codegen::link_executable(&object, target, &executable).unwrap();
            let actual = Command::new(executable).output().unwrap();
            assert_eq!(
                (actual.status.code(), actual.stdout, actual.stderr),
                (
                    expected.status.code(),
                    expected.stdout.clone(),
                    expected.stderr.clone()
                )
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
