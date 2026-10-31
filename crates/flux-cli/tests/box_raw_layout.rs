//! Box owns its payload while deallocation reads only raw layout metadata.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn box_raw_layout_matches_rustc_off_and_on() {
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = std::env::temp_dir().join(format!("flux-box-raw-native-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = repo.join("tests/run/box_raw_layout.rs");
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
    );
    let expected = Command::new(reference).output().unwrap();
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    for optimize in [false, true] {
        let executable = dir.join(if optimize { "flux-opt" } else { "flux" });
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimize {
            command.arg("-O");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = Command::new(executable).output().unwrap();
        assert_eq!(
            actual.status.code(),
            expected.status.code(),
            "{}",
            String::from_utf8_lossy(&actual.stderr)
        );
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn raw_layout_stable_api_matches_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-box-raw-api-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in [
        (
            "call",
            "fn main(){unsafe{let _=std::alloc::Layout::for_value_raw(std::ptr::null::<u8>());}}",
            true,
        ),
        (
            "item",
            "fn main(){let _f=std::alloc::Layout::for_value_raw::<u8>;}",
            true,
        ),
        (
            "pointer",
            "fn main(){let _f:unsafe fn(*const u8)->std::alloc::Layout=std::alloc::Layout::for_value_raw::<u8>;}",
            true,
        ),
        (
            "alias",
            "use core::alloc::Layout as L;fn main(){let _f=L::for_value_raw::<u8>;}",
            true,
        ),
        (
            "wrapper",
            "unsafe fn wrap(p:*const u8)->std::alloc::Layout{unsafe{std::alloc::Layout::for_value_raw(p)}}fn main(){}",
            true,
        ),
        (
            "macro",
            "macro_rules! raw{()=>{std::alloc::Layout::for_value_raw::<u8>};}fn main(){let _f=raw!();}",
            true,
        ),
        (
            "nested_names",
            "mod alloc{pub mod core{pub mod std{pub fn leak(){let _f=::core::alloc::Layout::for_value_raw::<u8>;}}}}fn main(){}",
            true,
        ),
        (
            "attribute",
            "#[unstable(feature=\"layout_for_ptr\",issue=\"69835\")]unsafe fn forged(){}fn main(){}",
            false,
        ),
        (
            "box_expired_reference",
            "fn main(){let _b;{let x=7;_b=Box::new(&x);}}",
            true,
        ),
        (
            "box_owned_inspector",
            "struct Inspect<'a>(&'a u32);impl Drop for Inspect<'_>{fn drop(&mut self){assert!(*self.0==7);}}fn main(){let _b;{let x=7;_b=Box::new(Inspect(&x));}}",
            false,
        ),
        (
            "may_dangle_attribute",
            "struct S<T>(*const T);unsafe impl<#[may_dangle] T> Drop for S<T>{fn drop(&mut self){}}fn main(){}",
            false,
        ),
        (
            "private_intrinsic",
            "fn main(){let _f=core::intrinsics::size_of_val::<u8>;}",
            false,
        ),
        (
            "stable_layout",
            "fn main(){let x=3u32;let l=std::alloc::Layout::for_value(&x);assert_eq!(l.size(),4);}",
            true,
        ),
        (
            "user_same_name",
            "struct Layout;impl Layout{unsafe fn for_value_raw(_: *const u8)->usize{7}}fn main(){let x=3;let n=unsafe{Layout::for_value_raw(&x)};assert_eq!(n,7);}",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let actual = Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("mir")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            actual.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
    // User attributes cannot acquire embedded-library privileges by naming
    // their crate core, alloc or std, or by using library-looking paths.
    for name in ["core", "alloc", "std"] {
        for (attribute, source) in [
            (
                "unstable",
                "#[unstable(feature=\"layout_for_ptr\",issue=\"69835\")]pub fn forged(){}",
            ),
            (
                "may_dangle",
                "pub struct S<T>(*const T);unsafe impl<#[may_dangle] T> Drop for S<T>{fn drop(&mut self){}}",
            ),
        ] {
            let path = dir.join(name).join(format!("src/{attribute}.rs"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, source).unwrap();
            let mut db = flux_hir::SourceDb::new();
            let root = db.load(&path).unwrap();
            let hir = flux_hir::lower_sources(
                &mut db,
                &[flux_hir::SourceCrate {
                    root,
                    name: flux_intern::Symbol::intern(name),
                    dependencies: vec![],
                }],
            );
            assert!(
                !hir.diagnostics.is_empty(),
                "forged {attribute} attribute admitted for {name}"
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unique_deallocation_never_borrows_the_payload() {
    let (db, _, hir) =
        flux_hir::lower_text("box-deallocation.rs", "fn main(){drop(Box::new(3u64));}");
    assert!(db.syntax_errors().is_empty());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let id = hir
        .items
        .functions
        .iter()
        .find_map(|(id, function)| {
            let imp = hir.items.impl_of(id)?;
            let ty = typed.sigs.impls.get(imp)?;
            match ty.kind() {
                flux_ty::TyKind::Adt(adt, _)
                    if adt.name.as_str() == "Unique"
                        && function.name.as_str() == "drop"
                        && hir.items.module_path(function.loc.module) == "alloc::boxed" =>
                {
                    Some(id)
                }
                _ => None,
            }
        })
        .expect("embedded allocation deallocator");
    let raw_layout = hir
        .items
        .functions
        .iter()
        .find_map(|(id, function)| {
            (function.name.as_str() == "for_value_raw"
                && hir.items.module_path(function.loc.module) == "core::alloc")
                .then_some(id)
        })
        .expect("core raw layout method");
    // A reference to generic T would require its already-dropped or moved
    // payload to remain a valid T. Borrowing self/NonNull/Layout is fine.
    for function in [id, raw_layout] {
        let body = flux_mir::build(&hir, &typed, function);
        for local in body.locals.values() {
            assert!(
                !matches!(local.ty.kind(), flux_ty::TyKind::Ref(_, t) if matches!(t.kind(), flux_ty::TyKind::Param(_))),
                "deallocation constructed a payload reference: {}",
                local.ty
            );
        }
    }
}
