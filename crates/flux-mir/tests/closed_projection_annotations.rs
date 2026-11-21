//! Selection lifetimes need no output rows when a projection is closed and region-free.
fn check(source: &str, accepted: bool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "flux-closed-projection-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let reference = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        reference.status.success(),
        accepted,
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    let mut errors = typed.diagnostics.clone();
    if errors.is_empty() {
        errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
    }
    assert_eq!(errors.is_empty(), accepted, "{errors:?}");
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn concrete_output_does_not_store_selection_lifetimes() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=u32;}fn f<'a>(_: &'a u8)->u32{let x:<S as Select<'a>>::Out=7;x}fn main(){assert_eq!(f(&0),7);}",
        true,
    );
}

#[test]
fn field_projection_proves_the_inherited_static_owner() {
    check(
        "trait Base:'static{} trait Select:Base{type Out;} struct S; impl Base for S{} impl Select for S{type Out=[u32;3];} struct Holder{value:<S as Select>::Out} fn main(){assert_eq!(Holder{value:[2,3,5]}.value[1],3);}",
        true,
    );
    check(
        "trait Base:'static{} trait Select:Base{type Out;} struct S<'a>(&'a u32); impl Base for S<'static>{} impl Select for S<'static>{type Out=[u32;3];} struct Holder<'a>{value:<S<'a> as Select>::Out} fn main(){}",
        false,
    );
}
#[test]
fn borrowed_output_cannot_escape() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=&'a u8;}fn f<'a>(x:&'a u8)->&'static u8{let y:<S as Select<'a>>::Out=x;y}fn main(){}",
        false,
    );
}
#[test]
fn selection_lifetime_still_must_be_declared() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=u32;}fn f(){let _: <S as Select<'missing>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn closed_output_does_not_prove_selection_lifetime_obligations() {
    check(
        "trait Select<'a>{type Out;}struct S;impl Select<'static> for S{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S as Select<'a>>::Out=7;}fn main(){}",
        false,
    );
}

#[test]
fn repeated_owner_lifetimes_match_the_impl_header() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S<'a> as Select<'a>>::Out=7;}fn main(){}",
        true,
    );
}
#[test]
fn repeated_owner_lifetimes_cannot_be_independent() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a,'b>(_: &'a u8,_: &'b u8){let _: <S<'a> as Select<'b>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn static_owner_lifetime_matches_a_static_impl() {
    check(
        "trait Select<'a>{type Out;}struct S;impl Select<'static> for S{type Out=u32;}fn f(){let _: <S as Select<'static>>::Out=7;}fn main(){}",
        true,
    );
}
#[test]
fn selected_impl_lifetime_bounds_are_not_erased() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a:'static>Select<'a> for S{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S as Select<'a>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn owner_and_self_lifetimes_must_match_together() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a,'b>(_: &'a u8,_: &'b u8){let _: <S<'a> as Select<'b>>::Out=7;}fn main(){}",
        false,
    );
}

#[test]
fn deferred_annotations_keep_local_type_and_alias_scopes() {
    check(
        include_str!("../../../tests/run/local_type_annotation_scopes.rs"),
        true,
    );
}

#[test]
fn local_projection_uses_the_impl_owner_lifetime() {
    check(
        include_str!("../../../tests/run/local_projection_owner.rs"),
        true,
    );
}

#[test]
fn independent_projection_outlives_bounds_follow_the_call() {
    let source = include_str!("../../../tests/run/projected_output_outlives.rs");
    check(source, true);
    check(
        &format!(
            "{source}\nfn requires_static<T: 'static>(_: T) {{}}\nfn escape<'a, 'b, F: Files<'a>>(id: &'b F::Id) {{ requires_static(*id); }}"
        ),
        false,
    );
}

#[test]
fn trait_default_projection_outlives_bounds_follow_the_call() {
    let source = include_str!("../../../tests/run/trait_default_projection_outlives.rs");
    check(source, true);
    check(
        &format!(
            "{source}\nfn escape(input: &u8) {{ <() as Parallel<_>>::first(Some(input).into_iter()); }}"
        ),
        false,
    );
}

#[test]
fn abbreviated_local_projection_needs_an_unambiguous_owner() {
    check(
        "trait Family<'a>{type Out;} fn f<'a,'b,C:Family<'a>+Family<'b>>(){let _: Option<C::Out>=None;} fn main(){}",
        false,
    );
}

#[test]
fn nested_abstract_output_does_not_gain_a_static_bound() {
    check(
        "use core::{marker::PhantomData,ptr::NonNull}; trait Family<'a>{type Out:?Sized;} struct Wrap<T:?Sized>(PhantomData<T>); fn f<'a,T:Family<'a>>(ptr:NonNull<Wrap<T::Out>>) {let _: &'static Wrap<T::Out>=unsafe{ptr.as_ref()};} fn main(){}",
        false,
    );
}
#[test]
fn local_alias_cannot_extend_a_borrow() {
    check(
        "fn bad<'a>(input:&'a u8)->&'static u8{struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let value:Alias<'static>=Hold(input);value.0}fn main(){}",
        false,
    );
}
#[test]
fn shadowed_alias_cannot_erase_its_static_annotation() {
    check(
        "fn bad<'a>(input:&'a u8){struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let _:Alias<'a>=Hold(input);{struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let _:Alias<'static>=Hold(input);}}fn main(){}",
        false,
    );
}

#[test]
fn closed_nested_outputs_and_type_only_families_keep_their_selection_proofs() {
    check(
        "trait F{type Out;}struct S;impl F for S{type Out=<u8 as std::ops::Add<u8>>::Output;}fn main(){let x:<S as F>::Out=7;assert_eq!(x,7);}",
        true,
    );
    check(
        "trait F{type Out<T:Copy>:Copy;}struct S;impl F for S{type Out<U:Copy>=U;}fn main(){let x:<S as F>::Out<u32>=7;assert_eq!(x,7);}",
        true,
    );
    check(
        "trait F{type Out<T:Copy>;}struct S;impl F for S{type Out<U:Copy>=u32;}fn main(){let _:<S as F>::Out<String>=7;}",
        false,
    );
}

#[test]
fn nested_projection_inputs_preserve_inner_selection_and_output_regions() {
    let source = include_str!("../../../tests/run/nested_projection_annotation.rs");
    check(source, true);
    check(
        &source.replace(
            "fn preserve_borrow<'a>(value: &'a u8) -> &'a u8",
            "fn preserve_borrow<'a>(value: &'a u8) -> &'static u8",
        ),
        false,
    );
    check(
        "trait Encode{type Abi;} impl Encode for &'static u8{type Abi=u32;} trait Word{type Prim;} impl Word for u32{type Prim=u32;} fn bad<'a>(_: &'a u8){let _: <<&'a u8 as Encode>::Abi as Word>::Prim=7;}fn main(){}",
        false,
    );
    check(
        "trait Encode{type Abi;} impl Encode for &'static u8{type Abi=u32;} struct Token<'a>(&'a u8); impl<'a> Encode for Token<'a>{type Abi=<&'a u8 as Encode>::Abi;} fn main(){let _: <Token<'_> as Encode>::Abi=7;}",
        false,
    );
    check(
        "trait Marker{} impl Marker for &'static u8{} trait Encode{type Abi;} impl<T:Marker> Encode for Option<T>{type Abi=u32;} fn bad<'a>(_: &'a u8){let _: <Option<&'a u8> as Encode>::Abi=7;}fn main(){}",
        false,
    );
}

#[test]
fn closed_callable_and_nested_selections_keep_their_header_obligations() {
    let source = include_str!("../../../tests/run/callable_projection_abi.rs");
    check(source, true);
    check(&source.replace("FnMut(u8)", "FnMut(bool)"), false);
    check(
        &format!(
            "{source}\ntrait StaticObject {{type Abi;}} impl<A, R> StaticObject for &mut (dyn FnMut(A) -> R + 'static) {{type Abi = u32;}} fn reject<'a>(_: &'a u8) {{let _ = identity::<<&'a mut (dyn FnMut(u8) -> u32 + 'a) as StaticObject>::Abi>(37);}}"
        ),
        false,
    );
    check(
        &format!(
            "{source}\ntrait Fixed<'a> {{type Value;}} impl Fixed<'static> for () {{type Value = u32;}} fn reject<'a>(_: &'a u8) {{let _ = identity::<<&'static <() as Fixed<'a>>::Value as Convert>::Abi>(37);}}"
        ),
        false,
    );
}

#[test]
fn impl_projection_outlives_applies_to_the_selected_output() {
    check(
        include_str!("../../../tests/run/projected_impl_outlives.rs"),
        true,
    );
    check(
        r#"
        trait Select<'a> { type Output; }
        struct Key;
        impl<'a> Select<'a> for Key { type Output = &'a u32; }
        struct Wrapper<'a>(&'a u32);
        impl<'a> Wrapper<'a>
        where
            <Key as Select<'a>>::Output: 'static,
        {
            fn new(value: &'a u32) -> Self { Self(value) }
        }
        fn main() {
            let value = 123;
            let _ = Wrapper::new(&value);
        }
        "#,
        false,
    );
}

#[test]
fn trait_impl_projection_lifetime_checks_declarations_and_calls() {
    let source = r#"
        trait Family<'a> { type View; }
        struct Key;
        impl<'a> Family<'a> for Key { type View = &'a u32; }
        struct Wrapper<'a, K: Family<'a>>(&'a u32, K);
        trait Mark {}
        impl<'a, K: Family<'a>> Mark for Wrapper<'a, K>
        where K::View: 'static {}
        fn marked<T: Mark>(_: T) {}
        static VALUE: u32 = 7;
        fn main() { marked(Wrapper(&VALUE, Key)); }
    "#;
    check(source, true);
    check(
        &source.replace(
            "marked(Wrapper(&VALUE, Key));",
            "let value = VALUE; marked(Wrapper(&value, Key));",
        ),
        false,
    );

    let concrete = source
        .replace(
            "impl<'a, K: Family<'a>> Mark for Wrapper<'a, K>",
            "impl<'a> Mark for Wrapper<'a, Key>",
        )
        .replace("K::View: 'static", "<Key as Family<'a>>::View: 'static");
    check(&concrete, false);
    check(
        &concrete.replace("impl<'a> Mark", "impl<'a: 'static> Mark"),
        true,
    );
}

#[test]
fn only_written_projection_lifetimes_count_for_elision() {
    check(
        include_str!("../../../tests/run/projected_return_elision.rs"),
        true,
    );
    check(
        r#"
        trait Family<'a> { type Owned; }
        fn from_owned<'a, T: Family<'a>>(_: &<T as Family<'a>>::Owned) -> &u32 { loop {} }
        fn main() {}
        "#,
        false,
    );
}

#[test]
fn captured_projection_uses_its_reference_bound() {
    let source = include_str!("../../../tests/run/projected_closure_capture.rs");
    check(source, true);
    check(
        &source.replace("Item = &'a u32", "Item = &'static u32"),
        false,
    );
}

#[test]
fn projected_static_bounds_survive_pointer_annotations() {
    let source = include_str!("../../../tests/run/yoke_projected_static_bound.rs");
    let invalid = source.replace(
        "let ptr: *const Self::Output =",
        "require_static::<Self::Output>(); let ptr: *const Self::Output =",
    );
    check(
        &format!("{invalid}\nfn require_static<T: 'static>() {{}}"),
        false,
    );
}

#[test]
fn qualified_callable_output_inherits_the_single_input_lifetime() {
    let source = include_str!("../../../tests/run/callable_projection_owner_elision.rs");
    check(source, true);
    check(
        &source
            .replace("FnOnce(&[u8])", "FnOnce(&[u8], &[u8])")
            .replace("f(&bytes)", "f(&bytes, &bytes)")
            .replace("|bytes|", "|bytes, _|"),
        false,
    );
}

#[test]
fn provider_signature_retains_the_static_projection_contract() {
    check(
        include_str!("../../../tests/run/provider_static_projection.rs"),
        true,
    );
}

#[test]
fn selected_nominal_defaults_refresh_outlives_subjects() {
    check(
        include_str!("../../../tests/run/nominal_projection_argument_outlives.rs"),
        true,
    );
}
