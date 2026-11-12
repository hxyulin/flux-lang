//! Mutable IndexMut receivers promote only literal empty arrays.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-empty-index-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        rustc.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}
#[test]
fn mutable_empty_slice_literal_can_escape() {
    compare(
        "empty",
        include_str!("../../../tests/run/empty_slice_index_mut.rs"),
        true,
    );
}
#[test]
fn mutable_array_storage_does_not_gain_promotion() {
    for (name, source) in [
        (
            "named",
            "fn f<'a>()->&'a mut[u8]{let mut data=[];&mut data[..]}fn main(){}",
        ),
        (
            "nonempty",
            "fn f<'a>()->&'a mut[u8]{&mut [1u8][..]}fn main(){}",
        ),
        ("zst", "fn f<'a>()->&'a mut[()]{&mut [()][..]}fn main(){}"),
    ] {
        compare(name, source, false);
    }
}
