//! Conversion-error and reference-range contracts used by typed parsers.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn parser_bound_apis_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-parser-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "infallible_error",
            "fn require<E:std::error::Error+ToString+Send+Sync+'static>(){}fn f(){require::<core::convert::Infallible>();require::<<i64 as TryFrom<i64>>::Error>();}",
            true,
        ),
        (
            "tryfromint_error",
            "fn require<E:std::error::Error+ToString+Send+Sync+'static>(){}fn f(){require::<<u8 as TryFrom<i64>>::Error>();require::<<i8 as TryFrom<u64>>::Error>();}",
            true,
        ),
        (
            "range_ref",
            "fn require<R:core::ops::RangeBounds<i64>>(_:R){}fn f(a:&i64,b:&i64){require(a..b);require(a..);require(..b);require(a..=b);require(..=b);}",
            true,
        ),
        (
            "range_ref_noncopy",
            "struct Key(u64);fn require<R:core::ops::RangeBounds<Key>>(_:R){}fn f(a:&Key,b:&Key){require(a..b);require(a..);require(..b);require(a..=b);require(..=b);}",
            true,
        ),
        (
            "range_tuple_unsized",
            "fn require<R:core::ops::RangeBounds<str>>(_:R){}fn f(){require((core::ops::Bound::Included(\"a\"),core::ops::Bound::Excluded(\"z\")));}",
            true,
        ),
        (
            "range_unsized_sized_boundary",
            "fn require<R:core::ops::RangeBounds<str>>(_:R){}fn f(){require(\"a\"..\"z\");}",
            false,
        ),
        (
            "range_ref_wrong_target",
            "fn require<R:core::ops::RangeBounds<i32>>(_:R){}fn f(a:&i64,b:&i64){require(a..b);}",
            false,
        ),
        (
            "range_bound_receiver_escape",
            "fn f<'a>(r:core::ops::Range<&'a i64>)->&'a i64{match<core::ops::Range<&i64>as core::ops::RangeBounds<i64>>::start_bound(&r){core::ops::Bound::Included(x)=>x,_=>panic!()}}",
            false,
        ),
        (
            "range_bound_short_escape",
            "fn f()->&'static i64{let a=1;let b=2;let r=&a..&b;match<core::ops::Range<&i64>as core::ops::RangeBounds<i64>>::start_bound(&r){core::ops::Bound::Included(x)=>x,_=>panic!()}}",
            false,
        ),
        (
            "error_not_blanket",
            "#[derive(Debug)]struct NotError;fn require<E:std::error::Error>(){}fn f(){require::<NotError>();}",
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

/// The concrete parser membership also discharges its structural auto bounds.
#[test]
fn typed_parser_conversion_error_bounds_reach_mir() {
    let source = "#[derive(Clone)]struct Parser<T>(core::marker::PhantomData<T>);trait Typed:Clone+Send+Sync+'static{}impl<T:TryFrom<i64>+Clone+Send+Sync+'static>Typed for Parser<T>where<T as TryFrom<i64>>::Error:std::error::Error+ToString+Send+Sync+'static{}fn need<T:Typed>(){}fn main(){need::<Parser<i64>>();need::<Parser<u8>>();}";
    let root = std::env::temp_dir().join(format!("flux-typed-parser-error-{}", std::process::id()));
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
    let (_, _, hir) = flux_hir::lower_text("typed_parser_error_contract.rs", source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
}
