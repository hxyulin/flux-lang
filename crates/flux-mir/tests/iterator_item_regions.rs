//! Tautological erased item types still need their original lifetime relationships.
#[test]
fn chained_reference_items_keep_lifetime_constraints() {
    for (name, bound, accepted) in [("matching", "'a:'b", true), ("escape", "'b:'a", false)] {
        let source = format!(
            "fn f<'a,'b>(a:&'a [u8],b:&'b [u8])->impl Iterator<Item=&'b u8> where {bound} {{a.iter().chain(b.iter())}}fn main(){{}}"
        );
        let root =
            std::env::temp_dir().join(format!("flux-chain-regions-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, &source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
