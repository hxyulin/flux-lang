//! Universal non-callable trait bounds preserve distinct binder slots.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn higher_ranked_trait_bounds_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-ranked-traits-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, valid) in [
        (
            "universal_owned",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} fn need<T:for<'a>Decode<'a>>(_:T){}fn main(){need(Owned);}",
            true,
        ),
        (
            "static_only_reject",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} fn need<T:for<'a>Decode<'a>>(_:T){}fn main(){need(Fixed);}",
            false,
        ),
        (
            "supertrait_blanket_owned",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} trait DecodeOwned:for<'a>Decode<'a>{}impl<T>DecodeOwned for T where T:for<'b>Decode<'b>{} fn need<T:DecodeOwned>(_:T){}fn main(){need(Owned);}",
            true,
        ),
        (
            "supertrait_blanket_static_reject",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} trait DecodeOwned:for<'a>Decode<'a>{}impl<T>DecodeOwned for T where T:for<'b>Decode<'b>{}fn need<T:DecodeOwned>(_:T){}fn main(){need(Fixed);}",
            false,
        ),
        (
            "borrowed_owned_reject",
            "trait Decode<'a>{}impl<'a>Decode<'a>for &'a str{}trait Owned:for<'a>Decode<'a>{}impl<T>Owned for T where T:for<'a>Decode<'a>{}fn need<T:Owned>(_:T){}fn f<'x>(s:&'x str){need(s);}fn main(){}",
            false,
        ),
        (
            "universal_assumption_use",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} fn inner<'a,T:Decode<'a>>(x:&T){let _=x.value();} fn outer<T:for<'a>Decode<'a>>(x:&T){inner(x);}fn main(){outer(&Owned);}",
            true,
        ),
        (
            "where_binder_form",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}} fn need<T>(_:T)where for<'a>T:Decode<'a>{}fn main(){need(Owned);}",
            true,
        ),
        (
            "multiple_distinct",
            "trait Pair<'a,'b>{}struct Any;impl<'a,'b>Pair<'a,'b>for Any{}fn need<T:for<'a,'b>Pair<'a,'b>>(_:T){}fn main(){need(Any);}",
            true,
        ),
        (
            "repeated_only_reject",
            "trait Pair<'a,'b>{}struct Same;impl<'a>Pair<'a,'a>for Same{}fn need<T:for<'a,'b>Pair<'a,'b>>(_:T){}fn main(){need(Same);}",
            false,
        ),
        (
            "cross_supertrait",
            "trait Decode<'a>{}struct Any;impl<'a>Decode<'a>for Any{}trait Owned:for<'a>Decode<'a>{}impl<T>Owned for T where T:for<'a>Decode<'a>{}trait More:Owned{}impl<T:Owned>More for T{}fn need<T:More>(_:T){}fn main(){need(Any);}",
            true,
        ),
        (
            "free_correlation",
            "trait Pair<'a,'b>{}struct Held<'b>(&'b u32);impl<'a,'b>Pair<'a,'b>for Held<'b>{}fn need<'b,T:for<'a>Pair<'a,'b>>(_:T){}fn f<'b>(x:&'b u32){need(Held(x));}fn main(){f(&7);}",
            true,
        ),
        (
            "free_static_reject",
            "trait Pair<'a,'b>{}struct Held<'b>(&'b u32);impl<'a>Pair<'a,'static>for Held<'static>{}fn need<'b,T:for<'a>Pair<'a,'b>>(_:T){}fn f<'b>(x:&'b u32){need(Held(x));}fn main(){}",
            false,
        ),
        (
            "duplicate_binder",
            "trait D<'a>{}fn f<T:for<'a,'a>D<'a>>(){}fn main(){}",
            false,
        ),
        (
            "shadow_binder",
            "trait D<'a>{}fn f<'a,T:for<'a>D<'a>>(){}fn main(){}",
            false,
        ),
        (
            "redundant_concrete_impl_bound",
            "trait D<'a>{}trait Owned:for<'a>D<'a>{}struct S;impl<'a>D<'a>for S{}impl Owned for S where for<'a> S:D<'a>, S:'static{}fn main(){}",
            true,
        ),
        (
            "false_supertrait_impl",
            "trait Decode<'de>{fn value(&self)->u32;} struct Owned;impl<'de> Decode<'de> for Owned{fn value(&self)->u32{7}} struct Fixed;impl Decode<'static> for Fixed{fn value(&self)->u32{1}}trait DecodeOwned:for<'a>Decode<'a>{}impl DecodeOwned for Fixed{}fn main(){}",
            false,
        ),
        (
            "stricter_method",
            "trait D<'a>{}trait Tr{fn f<T>()where T:D<'static>;}struct S;impl Tr for S{fn f<T>()where T:for<'a>D<'a>{}}fn main(){}",
            false,
        ),
        (
            "weaker_method",
            "trait D<'a>{}trait Tr{fn f<T>()where T:for<'a>D<'a>;}struct S;impl Tr for S{fn f<T>()where T:D<'static>{}}fn main(){}",
            true,
        ),
        (
            "renamed_weaker_method",
            "trait D<'a>{}trait Tr{fn f<U>()where U:for<'a>D<'a>;}struct S;impl Tr for S{fn f<V>()where V:D<'static>{}}fn main(){}",
            true,
        ),
        (
            "quantified_type_argument",
            "trait D<T>{}struct S;impl<'a>D<&'a u32>for S{}fn need<T:for<'a>D<&'a u32>>(_:T){}fn main(){need(S);}",
            true,
        ),
        (
            "static_type_argument_reject",
            "trait D<T>{}struct S;impl D<&'static u32>for S{}fn need<T:for<'a>D<&'a u32>>(_:T){}fn main(){need(S);}",
            false,
        ),
        (
            "callable_apit",
            "fn run(f:impl for<'a>FnOnce(&'a u32)->u32)->u32{f(&7)}fn read(x:&u32)->u32{*x}fn main(){assert_eq!(run(read),7);}",
            true,
        ),
        (
            "where_self_super",
            "trait D<'a>{}struct S;impl<'a>D<'a>for S{}trait Owned where for<'a>Self:D<'a>{}impl<T>Owned for T where for<'a>T:D<'a>{}fn need<T:Owned>(_:T){}fn main(){need(S);}",
            true,
        ),
        (
            "abstract_cross_super",
            "trait D<'a>{}trait Owned:for<'a>D<'a>{}trait More:Owned{}fn inner<'a,T:D<'a>>(_: &T){}fn outer<T:More>(x:&T){inner(x);}fn main(){}",
            true,
        ),
        (
            "unused_bound",
            "trait D<'a>{}struct S;impl<'a>D<'a>for S{}fn need<T:for<'a,'b>D<'a>>(_:T){}fn main(){need(S);}",
            true,
        ),
        (
            "assumption_repeated_reject",
            "trait Pair<'a,'b>{}fn need<T:for<'a,'b>Pair<'a,'b>>(){}fn bad<T:for<'a>Pair<'a,'a>>(){need::<T>();}fn main(){}",
            false,
        ),
        (
            "assumption_distinct_use",
            "trait Pair<'a,'b>{}fn need<'a,'b,T:Pair<'a,'b>>(){}fn use_all<T:for<'a,'b>Pair<'a,'b>>(){need::<T>();}fn main(){}",
            true,
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
            valid,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        if !matches!(name, "duplicate_binder" | "shadow_binder") {
            assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        }
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), valid, "flux {name}: {errors:?}");
    }
}

#[test]
fn higher_ranked_scalar_equality_is_supported() {
    let root = std::env::temp_dir().join(format!("flux-ranked-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = "trait D<'a>{type Out;}fn need<T:for<'a>D<'a,Out=u32>>(){}fn main(){}";
    let path = root.join("equality.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        errors.extend(flux_mir::check_file(&hir).diagnostics);
    }
    assert!(errors.is_empty(), "{errors:?}");
    std::fs::remove_dir_all(root).unwrap();
}
