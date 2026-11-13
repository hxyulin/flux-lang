//! Infallible conversions cannot narrow or change a signed value's range.
#[test]
fn invalid_nonzero_widening_matches_rustc() {
    for (name, text) in [
        (
            "narrow",
            "fn f(x:std::num::NonZeroU16)->std::num::NonZeroU8{x.into()}",
        ),
        (
            "same_width_signed",
            "fn f(x:std::num::NonZeroU8)->std::num::NonZeroI8{x.into()}",
        ),
        (
            "signed_unsigned",
            "fn f(x:std::num::NonZeroI8)->std::num::NonZeroU16{x.into()}",
        ),
        (
            "pointer_width_portability",
            "fn f(x:std::num::NonZeroU32)->std::num::NonZeroUsize{x.into()}",
        ),
    ] {
        let source = format!("{text}\nfn main(){{}}\n");
        let root =
            std::env::temp_dir().join(format!("flux-nz-widen-{name}-{}", std::process::id()));
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
