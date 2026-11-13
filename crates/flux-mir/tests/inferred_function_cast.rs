//! Contextual casts retain function signature, safety and binder checks.
#[test]
fn contextual_function_casts_match_rustc_rejections() {
    for (name, source) in [
        (
            "unsafe_to_safe",
            "unsafe fn f(){} fn main(){let _:fn()=f as _;}",
        ),
        (
            "wrong_abi",
            "extern \"C\" fn f(){} fn main(){let _:fn()=f as _;}",
        ),
        (
            "wrong_parameter",
            "fn f(_:u32){} fn main(){let _:fn(bool)=f as _;}",
        ),
        (
            "wrong_return",
            "fn f()->u32{1} fn main(){let _:fn()->bool=f as _;}",
        ),
        ("unconstrained", "fn f(){} fn main(){let _x=f as _;}"),
        (
            "implicit_static_to_universal",
            "fn f(x:&'static u32)->&'static u32{x} fn main(){let _:for<'a> fn(&'a u32)->&'a u32=f;}",
        ),
        (
            "static_to_universal",
            "fn f(x:&'static u32)->&'static u32{x} fn main(){let _:for<'a> fn(&'a u32)->&'a u32=f as _;}",
        ),
        (
            "pointer_unsafe_to_safe",
            "unsafe fn f(){} fn main(){let p:unsafe fn()=f;let _:fn()=p as _;}",
        ),
    ] {
        let dir =
            std::env::temp_dir().join(format!("flux-inferred-cast-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("main.rs");
        std::fs::write(&path, source).unwrap();
        let output = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(dir.join("out.rmeta"))
            .output()
            .unwrap();
        assert!(!output.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if typed.diagnostics.is_empty() {
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(!mir.diagnostics.is_empty(), "flux accepted {name}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
