//! Callback provenance is retained through normalization and shared signatures.
use flux_ty::regions::CallbackBinder::{Independent, SignatureBound, Unclassified};

#[test]
fn source_occurrences_and_resolved_tables_classify_callback_boundary() {
    let source = "
        trait Tr<T> { type Out; }
        impl<T> Tr<T> for () { type Out = T; }
        trait Life<'r> {}
        impl<'r> Life<'r> for () {}
        trait Scalar { type Out; }
        impl<T> Scalar for T { type Out = i32; }
        type Ref<'a> = &'a i32;
        type Erased<'a> = i32;
        fn static_predicate<'a>(_: &'a i32) where &'static i32: Copy {}
        fn plain<'a>(_: &'a i32) {}
        fn elided(_: &i32) {}
        fn unrelated<'a, T: Copy>(_: &'a T) {}
        fn binder<'a, F: for<'b> Fn(&'b i32)>(_: &'a i32, _: F) {}
        fn inline<'a: 'a>(_: &'a i32) {}
        fn where_bound<'a>(_: &'a i32) where 'a: 'a {}
        fn type_bound<'a, T: 'a>(_: &'a T) {}
        fn self_bound<'a>(_: &'a i32) where &'a i32: Copy {}
        fn argument<'a>(_: &'a i32) where (): Tr<&'a i32> {}
        fn lifetime_argument<'a>(_: &'a i32) where (): Life<'a> {}
        fn anon_const<'a>(_: &'a i32) where [i32; 1+1]: Copy {}
        fn nested_fn<'a>(_: &'a i32) where fn(&'a i32): Copy {}
        fn equality<'a>(_: &'a i32) where (): Tr<&'a i32, Out = &'a i32> {}
        fn nested<'a>(_: &'a i32) where (Option<&'a i32>, [&'a i32; 1]): Copy {}
        fn alias<'a>(_: &'a i32) where Ref<'a>: Copy {}
        fn erased_alias<'a>(_: &'a i32) where Erased<'a>: Copy {}
        fn projection<'a>(_: &'a i32) where <&'a i32 as Scalar>::Out: Copy {}
        struct Owner<'o>(&'o i32);
        impl<'o> Owner<'o> {
            fn owner_only<'a>(_: &'a i32) where &'o i32: Copy {}
            fn owner_and_own<'a>(_: &'a i32) where &'a i32: Copy {}
        }
        fn main() {}
    ";
    let (db, _, hir) = flux_hir::lower_text("metadata.rs", source);
    assert!(db.syntax_errors().is_empty());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    // Compile only: the metadata source includes deliberately gated trait
    // arguments and an abstract projection. Those earlier Flux errors remain.
    let dir = std::env::temp_dir().join(format!("flux-callback-metadata-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("metadata.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args([
            "--edition=2024",
            "--emit=metadata",
            "-Awarnings",
            "--out-dir",
        ])
        .arg(&dir)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    std::fs::remove_dir_all(dir).unwrap();
    let (shared, diagnostics) = flux_ty::signatures(&hir.items);
    assert!(
        diagnostics.iter().all(|d| d.message
            == "lifetimes in the generic arguments of bounds are not supported yet"
            || d.message == "the trait bound `<&i32 as Scalar>::Out: Copy` is not satisfied"
            || d.message == "lifetime parameters on traits are not supported yet"
            || d.message == "lifetimes in function pointer types are not supported yet"),
        "{diagnostics:?}"
    );
    let (fresh, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
    assert!(
        diagnostics.iter().all(|d| d.message
            == "lifetimes in the generic arguments of bounds are not supported yet"
            || d.message == "the trait bound `<&i32 as Scalar>::Out: Copy` is not satisfied"
            || d.message == "lifetime parameters on traits are not supported yet"
            || d.message == "lifetimes in function pointer types are not supported yet"),
        "{diagnostics:?}"
    );
    // Includes the frozen core layer, not just this source's functions.
    assert_eq!(format!("{shared:?}"), format!("{fresh:?}"));
    for (id, f) in hir
        .items
        .functions
        .iter()
        .skip(hir.items.core.functions as usize)
    {
        let r = &shared.regions.fns[id];
        let expected = match f.name.as_str() {
            "static_predicate" | "plain" | "elided" | "unrelated" | "binder" | "owner_only"
            | "main" | "Owner" => Independent,
            "anon_const" => Unclassified,
            _ => SignatureBound,
        };
        assert_eq!(r.callback_binder, expected, "{}: {r:?}", f.name);
        assert_eq!(r, &fresh.regions.fns[id]);
        if f.name.as_str() == "erased_alias" {
            assert!(!r.has_bounds);
            assert!(r.bound_selfs[0].is_empty());
            assert_eq!(r.callback_binder, SignatureBound);
        }
        if f.name.as_str() == "lifetime_argument" {
            assert_eq!(
                &*r.bound_traits[0].lifetimes,
                &[flux_ty::regions::Region::Param(0)]
            );
        }
        if f.name.as_str().starts_with("owner_") {
            assert_eq!(r.owner, 1);
            assert_eq!(r.names.len(), 2);
        }
    }
}
