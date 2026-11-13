//! Conditional removal must not hide errors in parameters which remain enabled.
#[test]
fn generic_parameter_cfg_controls_match_rustc() {
    for (name, text, accepted) in [
        (
            "native",
            include_str!("../../../tests/run/cfg_generic_parameters.rs"),
            true,
        ),
        (
            "active_duplicate",
            "struct S<#[cfg(all())] T,T>(T);fn main(){}",
            false,
        ),
        (
            "active_default_order",
            "struct S<#[cfg(all())] T=u8,U>(T,U);fn main(){}",
            false,
        ),
        (
            "active_missing_bound",
            "struct S<#[cfg_attr(all(),cfg(all()))] T:Missing>(T);fn main(){}",
            false,
        ),
        (
            "malformed_cfg",
            "struct S<#[cfg(not())] T>(T);fn main(){}",
            false,
        ),
        (
            "too_many_arguments",
            "struct S<#[cfg(any())] T,U>(U);fn main(){let _:S<u8,u16>;}",
            false,
        ),
        (
            "missing_active_lifetime",
            "struct S<#[cfg(any())] 'a,T>(&'a T);fn main(){}",
            false,
        ),
        (
            "binder",
            "fn f(_:for<#[cfg(any())] 'a,'b> fn(&'b u8)){}fn main(){}",
            true,
        ),
        (
            "disabled_outer_lifetime",
            "fn f<#[cfg(any())] 'a>(_:for<'a> fn(&'a u8)){}fn main(){}",
            true,
        ),
    ] {
        let root =
            std::env::temp_dir().join(format!("flux-generic-cfg-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, text).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, text);
        let typed = flux_ty::check_file(&hir);
        let success = db.syntax_errors().is_empty()
            && hir.diagnostics.is_empty()
            && typed.diagnostics.is_empty();
        assert_eq!(
            success,
            accepted,
            "flux {name}: syntax {:?}, hir {:?}, types {:?}",
            db.syntax_errors(),
            hir.diagnostics,
            typed.diagnostics
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
