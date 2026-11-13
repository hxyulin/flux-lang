//! Alias parameter types keep declaration scope and primitive restrictions.
#[test]
fn const_parameter_alias_rejections_match_rustc() {
    for (name, text) in [
        (
            "private_alias",
            "mod hidden{type Count=u8;}struct S<const N:hidden::Count>;",
        ),
        (
            "alias_arguments",
            "pub type Count=u8;struct S<const N:Count<u16>>;",
        ),
        ("primitive_arguments", "struct S<const N:u8<u16>>;"),
        ("float", "type Count=f32;struct S<const N:Count>;"),
        ("pointer", "type Count=*const u8;struct S<const N:Count>;"),
        ("generic", "type Id<T>=T;struct S<T,const N:Id<T>>(T);"),
        ("shadowed_primitive", "type u8=f32;struct S<const N:u8>;"),
        (
            "declaration_overflow",
            "mod a{type Count=u8;pub struct S<const N:Count>;}type Count=u16;fn f(){let _:a::S<256>;}",
        ),
        ("alias_cycle", "type A=B;type B=A;struct S<const N:A>;"),
    ] {
        let source = format!("{text}\nfn main(){{}}\n");
        let root =
            std::env::temp_dir().join(format!("flux-const-alias-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!reference.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert!(!typed.diagnostics.is_empty(), "flux accepted {name}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
