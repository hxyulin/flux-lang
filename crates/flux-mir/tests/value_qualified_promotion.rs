//! Associated declaration equalities retain the caller's item lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn value_qualified_promotion_matches_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-value-qualified-promotion-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "named_empty_variant",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static E{const X:E=E::None;&X}fn main(){}",
            true,
        ),
        (
            "literal_empty_variant",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static E{&E::None}fn main(){}",
            true,
        ),
        (
            "inactive_cell",
            "enum E{None,Cell(core::cell::Cell<u32>)}fn f()->&'static E{&E::None}fn main(){}",
            true,
        ),
        (
            "array_variants",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static[E;2]{&[E::None,E::None]}fn main(){}",
            true,
        ),
        (
            "struct_variant",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}struct S(E);fn f()->&'static S{&S(E::None)}fn main(){}",
            true,
        ),
        (
            "named_projection",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static u32{const X:(E,u32)=(E::None,73);&X.1}fn main(){}",
            true,
        ),
        (
            "active_drop",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static E{&E::Drop(D)}fn main(){}",
            false,
        ),
        (
            "named_active_drop",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}fn f()->&'static E{const X:E=E::Drop(D);&X}fn main(){}",
            false,
        ),
        (
            "enum_drop_impl",
            "enum E{None}impl Drop for E{fn drop(&mut self){}}fn f()->&'static E{&E::None}fn main(){}",
            false,
        ),
        (
            "active_cell",
            "enum E{None,Cell(core::cell::Cell<u32>)}fn f()->&'static E{&E::Cell(core::cell::Cell::new(7))}fn main(){}",
            false,
        ),
        (
            "factory_not_promoted",
            "enum E{None,Drop(D)}struct D;impl Drop for D{fn drop(&mut self){}}const fn make()->E{E::None}fn f()->&'static E{&make()}fn main(){}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}
