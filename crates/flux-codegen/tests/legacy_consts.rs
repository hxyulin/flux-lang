//! Bounded rustc legacy const arguments on the public unsafe NEON wrappers.
#![cfg(target_arch = "aarch64")]

fn front_end(text: &str) -> (flux_hir::Hir, flux_ty::TypedFile, flux_mir::FileMir) {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    (hir, typed, mir)
}

#[test]
fn legacy_const_runtime_program_lowers() {
    let (hir, typed, mir) = front_end(include_str!("../../../tests/run/neon_legacy_const_args.rs"));
    let module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    keel_ir::verify_module(&module).unwrap();
    keel_codegen::compile(&module, flux_codegen::Target::Aarch64AppleDarwin).unwrap();
}

#[test]
fn legacy_constants_do_not_become_runtime_expressions() {
    for (arg, expected) in [
        ("n", "attempt to use a non-constant value in a constant"),
        (
            "{ n += 1; n }",
            "legacy const expressions other than literals and constant paths are not supported yet",
        ),
        (
            "side_effect()",
            "legacy const expressions other than literals and constant paths are not supported yet",
        ),
        (
            "1 + 0",
            "legacy const expressions other than literals and constant paths are not supported yet",
        ),
        (
            "-N",
            "legacy const unary expressions other than negative integer literals are not supported yet",
        ),
        (
            "0.0",
            "legacy const expressions other than literals and constant paths are not supported yet",
        ),
    ] {
        let text = format!("use core::arch::aarch64::*; const N:i32=0; fn side_effect()->i32 {{ 0 }}
fn bad(a:uint64x2_t, mut n:i32) {{ unsafe {{ let _ = vgetq_lane_u64(a, {arg}); }} }} fn main() {{}}");
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(
            hir.diagnostics.iter().any(|d| d.message == expected),
            "{arg}: {:?}",
            hir.diagnostics
        );
        let body = hir
            .bodies
            .iter()
            .find(|(id, _)| hir.items.functions[*id].name.as_str() == "bad")
            .unwrap()
            .1;
        let body = &body.0;
        assert!(!body.exprs.iter().any(|(_, e)| matches!(e, flux_hir::Expr::Path(flux_hir::Res::Local(b)) if body.bindings[*b].name.as_str() == "n")));
        // Only the outer call exists; a rejected side effect has no HIR call.
        let calls: Vec<_> = body
            .exprs
            .iter()
            .filter_map(|(_, e)| match e {
                flux_hir::Expr::Call { args, .. } => Some(args.len()),
                _ => None,
            })
            .collect();
        assert_eq!(calls, [1]);
    }
}

#[test]
fn legacy_forms_keep_modern_arity_and_safety_rules() {
    for (call, expected) in [
        ("vgetq_lane_u64::<0>(a, 0)", "takes 1 argument"),
        ("vgetq_lane_u64(a, 0, 0)", "takes 1 argument"),
        ("vgetq_lane_u64()", "takes 1 argument"),
        ("vgetq_lane_u64(a)", "type annotations needed"),
        ("(vgetq_lane_u64)(a, 0)", "takes 1 argument"),
        (
            "{ let f = vgetq_lane_u64::<0>; f(a, 0) }",
            "takes 1 argument",
        ),
        (
            "{ let f:unsafe fn(uint64x2_t)->u64 = vgetq_lane_u64::<0>; f(a, 0) }",
            "takes 1 argument",
        ),
        ("vgetq_lane_u64(a, true)", "mismatched types"),
        ("vgetq_lane_u64(a, 0u32)", "mismatched types"),
        ("vgetq_lane_u64(a, -2147483649)", "literal out of range"),
    ] {
        let text = format!(
            "use core::arch::aarch64::*; unsafe fn bad(a:uint64x2_t) {{ unsafe {{ let _ = {call}; }} }} fn main() {{}}"
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{call}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message.contains(expected)),
            "{call}: {:?}",
            typed.diagnostics
        );
    }
    for statement in [
        "let _ = vgetq_lane_u64(a, 0);",
        "let _: fn(uint64x2_t)->u64 = vgetq_lane_u64::<0>;",
        "invoke(vgetq_lane_u64::<0>);",
    ] {
        let text = format!(
            "use core::arch::aarch64::*; fn invoke<F:Fn(uint64x2_t)->u64>(f:F) {{}} fn bad(a:uint64x2_t) {{ {statement} }} fn main() {{}}"
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &text);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        assert!(
            !flux_mir::check_file(&hir).diagnostics.is_empty(),
            "accepted {statement}"
        );
    }
}

#[test]
fn legacy_ranges_are_checked_after_substitution_and_reification() {
    for (name, ty, value, expected) in [
        (
            "vgetq_lane_u64",
            "uint64x2_t",
            "2",
            "lane index out of range",
        ),
        (
            "vgetq_lane_u64",
            "uint64x2_t",
            "-1",
            "lane index out of range",
        ),
        (
            "vget_lane_u64",
            "uint64x1_t",
            "1",
            "lane index out of range",
        ),
        (
            "vshrn_n_u16",
            "uint16x8_t",
            "0",
            "shift amount out of range",
        ),
        (
            "vshrn_n_u16",
            "uint16x8_t",
            "9",
            "shift amount out of range",
        ),
        (
            "vshrn_n_u16",
            "uint16x8_t",
            "-1",
            "shift amount out of range",
        ),
    ] {
        for call in [
            format!("{name}(a, {value})"),
            format!("generic::<{value}>(a)"),
            format!("{{ let f:fn({ty})->() = generic::<{value}>; f(a) }}"),
        ] {
            let text = format!(
                "use core::arch::aarch64::*;
fn generic<const N:i32>(a:{ty}) {{ unsafe {{ let _ = {name}(a,N); }} }}
fn main() {{ unsafe {{ let a = *([0u64;2].as_ptr() as *const {ty}); let _ = {call}; }} }}"
            );
            let (hir, typed, mir) = front_end(&text);
            let errors = flux_codegen::lower_module(&hir, &typed, &mir)
                .err()
                .unwrap_or_default();
            assert!(
                errors.iter().any(|d| d.message.contains(expected)),
                "{call}: {errors:?}"
            );
        }
    }
}

#[test]
fn user_functions_cannot_opt_in_by_name_or_attribute() {
    let (hir, typed, mir) = front_end(
        "fn vgetq_lane_u64(a:u64, n:u64)->u64 { a+n } fn main() { let _ = vgetq_lane_u64(1,2); }",
    );
    flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let (_, _, hir) = flux_hir::lower_text(
        "main.rs",
        "#[rustc_legacy_const_generics(1)] fn own<const N:i32>(a:u64) {} fn main() {}",
    );
    assert!(!hir.diagnostics.is_empty());
}
