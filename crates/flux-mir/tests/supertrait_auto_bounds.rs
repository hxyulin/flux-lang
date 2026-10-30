//! Structural auto-trait supergoals use original impl-header assumptions.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn structural_auto_supertraits_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-super-auto-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "missing_static_supertrait",
            "#[derive(Clone)]struct W<T>(core::marker::PhantomData<T>);trait Typed:Clone+Send+Sync+'static{}impl<T:Clone+Send+Sync>Typed for W<T>{}",
            false,
        ),
        (
            "selected_type_impl_positive",
            "struct W<T>(core::marker::PhantomData<T>);unsafe impl<T:Sync>Send for W<T>{}trait Typed:Send{}impl<T:Send+Sync>Typed for W<T>{}",
            true,
        ),
        (
            "nested_lifetime_auto_impl",
            "struct Borrow<'a>(&'a i32);struct W<B,T>(B,T);unsafe impl<T:Send>Send for W<Borrow<'_>,T>{}trait Typed:Send{}impl<'a,T:Send>Typed for W<Borrow<'a>,T>{}",
            true,
        ),
        (
            "nested_lifetime_auto_impl_bound",
            "struct Borrow<'a>(&'a i32);struct W<B,T>(B,T);unsafe impl<T:Send>Send for W<Borrow<'_>,T>{}trait Typed:Send{}impl<'a,T>Typed for W<Borrow<'a>,T>{}",
            false,
        ),
        (
            "permuted_nested_lifetimes",
            "struct Borrow<'a,'b>(&'a i32,&'b i32);struct W<'a,B>(&'a i32,B);unsafe impl<'a,'b,'c>Send for W<'c,Borrow<'b,'a>>{}trait Typed:Send{}impl<'a,'b,'c>Typed for W<'a,Borrow<'b,'c>>{}",
            true,
        ),
        (
            "phantom_send",
            "struct W<T>(core::marker::PhantomData<T>);trait Typed:Send{}impl<T:Send>Typed for W<T>{}",
            true,
        ),
        (
            "phantom_sync",
            "struct W<T>(core::marker::PhantomData<T>);trait Typed:Sync{}impl<T:Sync>Typed for W<T>{}",
            true,
        ),
        (
            "owned_send",
            "struct W<T>(T);trait Typed:Send{}impl<T:Send>Typed for W<T>{}",
            true,
        ),
        (
            "tuple_fields",
            "struct W<T>((T,u32));trait Typed:Send+Sync{}impl<T:Send+Sync>Typed for W<T>{}",
            true,
        ),
        (
            "array_fields",
            "struct W<T>([T;2]);trait Typed:Send+Sync{}impl<T:Send+Sync>Typed for W<T>{}",
            true,
        ),
        (
            "nested_fields",
            "struct Inner<T>(T);struct W<T>(Inner<T>);trait Typed:Send+Sync{}impl<T:Send+Sync>Typed for W<T>{}",
            true,
        ),
        (
            "header_projected_field",
            "trait Family{type Out;}struct W<T:Family>(T::Out);trait Typed:Send{}impl<T:Family>Typed for W<T>where T::Out:Send{}",
            true,
        ),
        (
            "missing_send",
            "struct W<T>(T);trait Typed:Send{}impl<T>Typed for W<T>{}",
            false,
        ),
        (
            "raw_pointer_leaf",
            "struct W<T>(*const T);trait Typed:Send{}impl<T:Send>Typed for W<T>{}",
            false,
        ),
        (
            "unsafe_cell_leaf",
            "struct W<T>(core::cell::UnsafeCell<T>);trait Typed:Sync{}impl<T:Sync>Typed for W<T>{}",
            false,
        ),
        (
            "rc_leaf",
            "struct W<T>(std::rc::Rc<T>);trait Typed:Send{}impl<T:Send>Typed for W<T>{}",
            false,
        ),
        (
            "explicit_impl_suppresses_structure",
            "struct W<T>(core::marker::PhantomData<T>);unsafe impl<T:Sync>Send for W<T>{}trait Typed:Send{}impl<T:Send>Typed for W<T>{}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}

/// Bounded-lifetime auto impl admission remains explicit. The lower-level
/// proof unit test separately checks that its metadata is never discarded.
#[test]
fn lifetime_auto_impl_boundary_is_preserved() {
    let source = "struct Guard<T>(core::marker::PhantomData<T>);unsafe impl<T:'static>Send for Guard<T>{}struct W<T>(Guard<T>);trait Typed:Send{}impl<T:'static>Typed for W<T>{}fn main(){}";
    let root = std::env::temp_dir().join(format!(
        "flux-auto-lifetime-boundary-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("case.rmeta"))
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let typed = flux_mir::check_file(&hir);
    assert!(
        typed.diagnostics.iter().any(|d| d.message
            == "lifetime specialization and bounds in auto trait impls are not supported yet"),
        "{:?}",
        typed.diagnostics
    );
}

#[test]
fn deep_auto_trait_signature_proof() {
    for (leaf, accepted) in [("u32", true), ("*const ()", false)] {
        let mut source = format!("struct Layer0({leaf});");
        for i in 1..45 {
            source.push_str(&format!("struct Layer{i}(Layer{});", i - 1));
        }
        source.push_str("trait Transfer:Send+Sync{}impl Transfer for Layer44{}fn main(){}");
        let (_, _, hir) = flux_hir::lower_text("deep_auto.rs", &source);
        let (_, errors) = flux_ty::signatures_checked(&hir.items, true, None);
        assert_eq!(errors.is_empty(), accepted, "{errors:?}");
        if accepted {
            let typed = flux_mir::check_file(&hir);
            let mir = flux_mir::build_file(&hir, &typed);
            assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        }
    }
}
