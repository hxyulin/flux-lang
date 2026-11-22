//! Tautological erased item types still need their original lifetime relationships.
#[test]
fn chained_reference_items_keep_lifetime_constraints() {
    for (name, expr, item, bound, accepted) in [
        ("matching", "a.iter().chain(b.iter())", "u8", "'a:'b", true),
        ("escape", "a.iter().chain(b.iter())", "u8", "'b:'a", false),
        (
            "once_matching",
            "core::iter::once(a).chain(core::iter::once(b))",
            "[u8]",
            "'a:'b",
            true,
        ),
        (
            "once_escape",
            "core::iter::once(a).chain(core::iter::once(b))",
            "[u8]",
            "'b:'a",
            false,
        ),
    ] {
        let source = format!(
            "fn f<'a,'b>(a:&'a [u8],b:&'b [u8])->impl Iterator<Item=&'b {item}> where {bound} {{{expr}}}fn main(){{}}"
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
