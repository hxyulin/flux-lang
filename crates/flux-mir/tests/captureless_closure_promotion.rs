#[test]
fn captured_or_mutable_closures_cannot_escape() {
    for (name, source) in [
        (
            "borrowed",
            "fn make<'a>(x:&'a u32)->&'static dyn Fn()->u32{&||*x}fn main(){}",
        ),
        (
            "moved",
            "fn make(x:String)->&'static dyn Fn()->usize{&move||x.len()}fn main(){}",
        ),
        (
            "mutable",
            "fn make()->&'static mut dyn FnMut(){&mut||{}}fn main(){}",
        ),
    ] {
        let root = std::env::temp_dir().join(format!(
            "flux-promoted-closure-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!oracle.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert!(!errors.is_empty(), "flux accepted {name}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
