//! Every integer nonzero wrapper rejects zero constants.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn nonzero_family_constants_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-nonzero-family-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for wrapper in [
        "NonZeroI8",
        "NonZeroI16",
        "NonZeroI32",
        "NonZeroI64",
        "NonZeroI128",
        "NonZeroIsize",
        "NonZeroU8",
        "NonZeroU16",
        "NonZeroU32",
        "NonZeroU64",
        "NonZeroU128",
        "NonZeroUsize",
    ] {
        for (case, accepted) in [("checked", true), ("unchecked_zero", false)] {
            let name = format!("{wrapper}_{case}");
            let expression = if accepted {
                format!("const X: Option<core::num::{wrapper}> = core::num::{wrapper}::new(0);")
            } else {
                format!(
                    "const X: core::num::{wrapper} = unsafe {{ core::num::{wrapper}::new_unchecked(0) }};"
                )
            };
            if !case_filter::selected(&name) {
                continue;
            }
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
    }
    std::fs::remove_dir_all(root).unwrap();
}
