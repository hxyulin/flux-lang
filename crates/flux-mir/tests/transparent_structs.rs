//! Transparent declarations retain their field-layout contract for every instantiation.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn transparent_declarations_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-transparent-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, declaration, accepted) in [
        (
            "empty_enum",
            "enum Void {} #[repr(transparent)] struct W(Void,u32);",
            true,
        ),
        ("empty", "#[repr(transparent)] struct W;", true),
        ("units", "#[repr(transparent)] struct W((),());", true),
        (
            "marker",
            "#[repr(transparent)] struct W<T>(u32,core::marker::PhantomData<T>);",
            true,
        ),
        (
            "generic_payload",
            "#[repr(transparent)] struct W<T>((),T,core::marker::PhantomData<T>);",
            true,
        ),
        (
            "array_payload",
            "#[repr(transparent)] struct W<T>([u8;4],core::marker::PhantomData<T>);",
            true,
        ),
        (
            "nested_marker",
            "struct M<T>(core::marker::PhantomData<T>);#[repr(transparent)] struct W<T>(u64,M<T>);",
            true,
        ),
        (
            "zero_array",
            "#[repr(transparent)] struct W([u8;0],u32);",
            true,
        ),
        (
            "generic_zero_array",
            "#[repr(transparent)] struct W<T>([T;0]);",
            true,
        ),
        (
            "aligned_only",
            "#[repr(align(32))] struct A;#[repr(transparent)] struct W(A,());",
            true,
        ),
        (
            "sized_prefix",
            "#[repr(transparent)] struct W<T:?Sized>((),T);",
            true,
        ),
        (
            "two_payloads",
            "#[repr(transparent)] struct W(u32,u8);",
            false,
        ),
        (
            "aligned_marker",
            "#[repr(align(8))] struct A;#[repr(transparent)] struct W(u32,A);",
            false,
        ),
        (
            "generic_marker",
            "#[repr(transparent)] struct W<T>(u32,T);",
            false,
        ),
        (
            "two_generics",
            "#[repr(transparent)] struct W<T,U>(T,U);",
            false,
        ),
        (
            "generic_zero_array_alignment",
            "#[repr(transparent)] struct W<T>(u32,[T;0]);",
            false,
        ),
        (
            "sized_dst_prefix",
            "#[repr(transparent)] struct W<T:?Sized>(u8,T);",
            false,
        ),
        (
            "nonzero_array",
            "#[repr(transparent)] struct W([u8;1],u32);",
            false,
        ),
        ("repr_c", "#[repr(C,transparent)] struct W(u32);", false),
        (
            "repr_align",
            "#[repr(transparent,align(8))] struct W(u32);",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{declaration} fn main() {{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut diagnostics = hir.diagnostics.clone();
        if diagnostics.is_empty() {
            let typed = flux_mir::check_file(&hir);
            diagnostics.extend(typed.diagnostics.iter().cloned());
            if diagnostics.is_empty() {
                diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(diagnostics.is_empty(), accepted, "{name}: {diagnostics:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
