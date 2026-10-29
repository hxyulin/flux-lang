//! Lint-only generated metadata grants no exemptions from semantic checks.
use std::process::Command;

#[test]
fn metadata_preserves_impl_semantics() {
    let root =
        std::env::temp_dir().join(format!("flux-generated-semantics-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (source, expected) in [
        (
            "trait T{fn value(&self)->u32;}struct S;#[automatically_derived]impl T for S{fn value(&self)->u32{7}}",
            true,
        ),
        (
            "trait T{fn value(&self)->u32;}struct S;#[automatically_derived]impl T for S{fn value(&self)->u32{false}}",
            false,
        ),
        (
            "trait T{fn value(&self)->u32;}struct S;#[automatically_derived]impl T for S{}",
            false,
        ),
        (
            "trait T{}struct S;#[automatically_derived]impl T for S{}impl T for S{}",
            false,
        ),
        (
            "trait T{}struct S;#[automatically_derived]unsafe impl T for S{}",
            false,
        ),
        ("#[automatically_derived]fn bad()->u32{false}", false),
    ] {
        let path = root.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--crate-type=lib", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            expected,
            "{source}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut accepted = hir.diagnostics.is_empty();
        if accepted {
            let typed = flux_mir::check_file(&hir);
            accepted = typed.diagnostics.is_empty();
            if accepted {
                accepted = flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
            }
        }
        assert_eq!(accepted, expected, "{source}: {:?}", hir.diagnostics);
    }
    std::fs::remove_dir_all(root).unwrap();
}
