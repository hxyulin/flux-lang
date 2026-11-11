//! Nightly syntax is preserved by parsing but never silently given stable semantics.
#[test]
fn unsupported_nightly_semantics_are_reported() {
    for (source, message) in [
        (
            "trait T { fn f(); } impl T for u8 { default fn f() {} }",
            "specialization is not supported yet",
        ),
        (
            "trait T {} default impl T for u8 {}",
            "specialization is not supported yet",
        ),
        ("fn f(box x: u8) {}", "box patterns are not supported yet"),
        (
            "macro_rules! make { () => { impl u8 { default fn f() {} } } } make!();",
            "specialization is not supported yet",
        ),
        (
            "macro_rules! pat { () => { box x } } fn f() { let pat!() = 0; }",
            "box patterns are not supported yet",
        ),
    ] {
        let (_, _, hir) = flux_hir::lower_text("nightly.rs", source);
        assert!(
            hir.diagnostics.iter().any(|d| d.message == message),
            "{source}: {:?}",
            hir.diagnostics
        );
    }
}
