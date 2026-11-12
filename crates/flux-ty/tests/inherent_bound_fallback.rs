//! Inherent lookup skips impossible impl bounds without hiding their errors.
#[test]
fn missing_bounds_and_ambiguity_stay_errors() {
    for (name, text) in [
        (
            "unmet_only",
            "trait Limited{} struct W<T>(T); impl<T:Limited> W<T>{fn value(&self){}} fn f<T>(x:&W<T>){x.value();}",
        ),
        (
            "ambiguous_traits",
            "trait Limited{} struct W<T>(T); impl<T:Limited> W<T>{fn value(&self){}} trait A{fn value(&self);}trait B{fn value(&self);}impl<T>A for W<T>{fn value(&self){}}impl<T>B for W<T>{fn value(&self){}}fn f<T>(x:&W<T>){x.value();}",
        ),
    ] {
        let source = format!("{text}\nfn main(){{}}\n");
        let root =
            std::env::temp_dir().join(format!("flux-inherent-{name}-{}", std::process::id()));
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
