#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn modules_coexist_with_bare_primitive_types() {
    let dir = std::env::temp_dir().join(format!("flux-primitive-namespace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in [
        (
            "local",
            "mod str{}mod u32{}fn f(_: &str,_:u32){}fn main(){f(\"x\",3);}",
            true,
        ),
        (
            "import",
            "use core::str;fn f(x:&str)->&str{x}fn main(){assert_eq!(f(\"x\"),\"x\");}",
            true,
        ),
        (
            "renamed",
            "use core::str as u8;fn f(_:u8){}fn main(){f(3);}",
            true,
        ),
        (
            "float_module",
            "mod f64{}fn f(_:f64){}fn main(){f(1.0);}",
            true,
        ),
        (
            "struct_shadows",
            "struct str(u8);fn f(x:str)->u8{x.0}fn main(){assert_eq!(f(str(3)),3);}",
            true,
        ),
        (
            "parameter_shadows",
            "mod str{}fn f<str>(x:str)->str{x}fn main(){let _:u8=f(3);}",
            true,
        ),
        (
            "alias_not_primitive",
            "use core::str as Text;fn f(_: &Text){}fn main(){}",
            false,
        ),
        (
            "qualified",
            "mod str{}fn f(_: &crate::str){}fn main(){}",
            false,
        ),
        (
            "primitive_args",
            "use core::str;fn f(_: &str<u8>){}fn main(){}",
            false,
        ),
        (
            "nominal_shadow_wrong",
            "struct u32;fn f(_:u32){}fn main(){f(3);}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join("test.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(db.syntax_errors().is_empty());
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_ty::check_file(&hir).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepts, "Flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
