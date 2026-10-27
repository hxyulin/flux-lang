//! Associated trait lookup retains obligations until expected types resolve Self.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn associated_context_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-associated-context-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "concrete_default",
            "fn take(_:std::hash::RandomState){}fn f(){take(<_>::default());}",
            true,
        ),
        (
            "generic_default",
            "fn take<T>(_:T){}fn f<T:Default>(){take::<T>(<_>::default());}",
            true,
        ),
        (
            "return_default",
            "fn f<T:Default>()->T{<_>::default()}",
            true,
        ),
        (
            "custom_expected",
            "trait Make{fn make()->Self;}impl Make for u32{fn make()->u32{7}}fn f(){let n:u32=<_>::make();assert_eq!(n,7);}",
            true,
        ),
        (
            "custom_generic",
            "trait Make{fn make()->Self;}fn f<T:Make>()->T{<_>::make()}",
            true,
        ),
        (
            "nested_result",
            "trait Make:Sized{fn make()->Option<Self>;}impl Make for u32{fn make()->Option<Self>{Some(7)}}fn f(){let n:Option<u32>=<_>::make();assert_eq!(n,Some(7));}",
            true,
        ),
        (
            "arguments_infer_self",
            "trait Make{fn make(x:Self)->Self;}impl Make for u32{fn make(x:Self)->Self{x}}fn f(){let n=<_>::make(7u32);assert_eq!(n,7);}",
            true,
        ),
        (
            "associated_const",
            "trait Make:Sized{const MADE:Self;}impl Make for u32{const MADE:Self=7;}fn f(){let n:u32=<_>::MADE;assert_eq!(n,7);}",
            true,
        ),
        (
            "wrong_expected_bound",
            "trait Make{fn make()->Self;}impl Make for u32{fn make()->Self{7}}fn f(){let n:bool=<_>::make();}",
            false,
        ),
        (
            "missing_generic_bound",
            "trait Make{fn make()->Self;}fn f<T>()->T{<_>::make()}",
            false,
        ),
        (
            "unconstrained_self",
            "trait Make{fn make()->Self;}impl Make for u32{fn make()->Self{7}}fn f(){let n=<_>::make();}",
            false,
        ),
        (
            "ambiguous_traits",
            "trait A{fn make()->Self;}trait B{fn make()->Self;}impl A for u32{fn make()->Self{7}}impl B for u32{fn make()->Self{8}}fn f(){let n:u32=<_>::make();}",
            false,
        ),
        (
            "return_does_not_identify_self",
            "trait Make{fn make()->u32;}impl Make for (){fn make()->u32{7}}fn f(){let n:u32=<_>::make();}",
            false,
        ),
        (
            "unknown_method_receiver",
            "trait Make{fn make(&self)->u32;}impl Make for (){fn make(&self)->u32{7}}fn f(){let x=Default::default();let n=x.make();}",
            false,
        ),
        (
            "qualified_bound_not_imported",
            "mod m{pub trait Make{fn make()->Self;}}fn f<T:m::Make>()->T{<_>::make()}",
            false,
        ),
        (
            "qualified_impl_not_imported",
            "mod m{pub trait Make{fn make()->Self;}}struct S;impl m::Make for S{fn make()->Self{S}}fn f()->S{<_>::make()}",
            false,
        ),
        (
            "distinct_trait_impls_still_ambiguous",
            "trait A{fn make()->Self;}trait B{fn make()->Self;}impl A for u32{fn make()->Self{7}}impl B for bool{fn make()->Self{true}}fn f(){let n:u32=<_>::make();}",
            false,
        ),
        (
            "blanket_conditional",
            "trait Make{fn make()->Self;}impl<T:Default>Make for T{fn make()->Self{T::default()}}fn f<T:Default>()->T{<_>::make()}",
            true,
        ),
        (
            "generic_trait_argument",
            "trait Make<A>{fn make(x:A)->Self;}impl Make<u8>for u32{fn make(x:u8)->Self{u32::from(x)}}fn f()->u32{<_>::make(7u8)}",
            true,
        ),
        (
            "wrong_trait_argument",
            "trait Make<A>{fn make(x:A)->Self;}impl Make<u8>for u32{fn make(x:u8)->Self{u32::from(x)}}fn f()->u32{<_>::make(false)}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
