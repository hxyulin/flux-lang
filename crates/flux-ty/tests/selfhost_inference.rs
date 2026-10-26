//! Inference constraints that occur in the unchanged type checker itself.
#[test]
fn context_precedes_method_defaults_and_deferred_comparisons() {
    let root = std::env::temp_dir().join(format!("flux-selfhost-inference-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "slice_default",
            "fn main() { let xs: Option<Box<[u32]>> = Some(vec![3u32,4].into_boxed_slice()); let ys: &[u32] = xs.as_ref().map_or(&[], |v| v); assert_eq!(ys.len(), 2); }",
        ),
        (
            "slice_nested",
            "fn choose(xs: &Option<Box<[u32]>>) -> &[u32] { xs.as_ref().map_or(&[], |v| v) } fn main() {}",
        ),
        (
            "late_comparison",
            "#[derive(PartialEq, Eq, Clone, Copy)] enum Region { Static, Param(u32) } fn main() { let reaches = |a,b| { let mut seen=vec![a]; let a=seen[0]; a==b || a==Region::Static }; assert!(reaches(Region::Param(2), Region::Param(2))); }",
        ),
        (
            "associated_len",
            "fn main() { let xs: Option<&[u32]> = Some(&[3,4]); assert_eq!(xs.map_or(0, <[u32]>::len),2); let f = <[u8]>::len; assert_eq!(f(&[1,2,3]),3); }",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
