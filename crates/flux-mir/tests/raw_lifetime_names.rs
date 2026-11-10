//! Raw lifetime spellings preserve identity and ordinary borrow obligations.
#[test]
fn raw_lifetime_rejections_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-raw-lifetimes-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        ("duplicate", "fn f<'name,'r#name>(){} fn main(){}"),
        (
            "shadow",
            "fn f<'name>(_:for<'r#name> fn(&'r#name str)){} fn main(){}",
        ),
        (
            "escape",
            "fn f<'r#name>()->&'name str{let s=String::from(\"local\");&s} fn main(){}",
        ),
        ("static_parameter", "fn f<'r#static>(){} fn main(){}"),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!reference.status.success(), "rustc must reject {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert!(!errors.is_empty(), "flux must reject {name}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
