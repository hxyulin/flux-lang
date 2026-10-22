//! Independent exhaustive alternatives must not enumerate their product.

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

fn program(columns: usize) -> flux_hir::Hir {
    let types = vec!["bool"; columns].join(",");
    let patterns = vec!["true | false"; columns].join(",");
    let text =
        format!("fn f(x: ({types})) -> i32 {{ match x {{ ({patterns}) => 1 }} }} fn main() {{}}");
    let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    hir
}

#[test]
fn independent_exhaustive_boolean_columns() {
    // The old algorithm visits about a million branches at 20 columns.
    scaling::assert_scales("exhaustive boolean alternatives", 5, program, |hir| {
        let typed = flux_ty::check_file(hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    });
}

#[test]
fn alternatives_preserve_restrictions_and_guards() {
    for text in [
        "fn f(x: (bool, bool)) { match x { (true | false, true) => () } } fn main() {}",
        "fn f(x: bool) { match x { true | false if x => () } } fn main() {}",
        "fn f(x: bool) { match x { false | false => () } } fn main() {}",
    ] {
        let (_, _, hir) = flux_hir::lower_text("main.rs", text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("non-exhaustive patterns")),
            "{text}: {:?}",
            typed.diagnostics
        );
    }
    for pattern in [
        "(false | false) | true",
        "true | (false | false)",
        "_ | false",
    ] {
        let text = format!("fn f(x: bool) {{ match x {{ {pattern} => () }} }} fn main() {{}}");
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_ty::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{pattern}: {:?}",
            typed.diagnostics
        );
    }
}
