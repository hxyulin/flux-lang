//! NonZeroU64 preserves scalar validity during constant evaluation.
#[test]
fn nonzero_u64_constants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-nonzero-u64-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expression, accepted) in [
        (
            "checked_zero",
            "const X: Option<core::num::NonZeroU64> = core::num::NonZeroU64::new(0);",
            true,
        ),
        (
            "checked_max",
            "const X: Option<core::num::NonZeroU64> = core::num::NonZeroU64::new(u64::MAX);",
            true,
        ),
        (
            "unchecked",
            "const X: core::num::NonZeroU64 = unsafe { core::num::NonZeroU64::new_unchecked(1 << 63) };",
            true,
        ),
        (
            "invalid_zero",
            "const X: core::num::NonZeroU64 = unsafe { core::num::NonZeroU64::new_unchecked(0) };",
            false,
        ),
        (
            "invalid_payload",
            "const X: Option<core::num::NonZeroU64> = Some(unsafe { core::num::NonZeroU64::new_unchecked(0) });",
            false,
        ),
    ] {
        let source = format!("{expression} fn main() {{ let _ = X; }}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepted,
            "{name}: {:?}",
            mir.diagnostics
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
