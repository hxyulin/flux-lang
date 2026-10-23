//! Associated bounds must hold under universal impl header assumptions.
//! Invalid sources are compiled for diagnostics only and never executed.
use std::process::Command;

#[test]
fn associated_bounds_match_rustc() {
    let cases = [
        (
            "associated_bound_region_bad",
            include_str!("data/associated_bounds/associated_bound_region_bad.rs"),
            false,
        ),
        (
            "associated_bound_escape_inferred",
            include_str!("data/associated_bounds/associated_bound_escape_inferred.rs"),
            false,
        ),
        (
            "associated_bound_escape_callback",
            include_str!("data/associated_bounds/associated_bound_escape_callback.rs"),
            false,
        ),
        (
            "associated_bound_escape",
            include_str!("data/associated_bounds/associated_bound_escape.rs"),
            false,
        ),
        (
            "associated_bound_positive",
            include_str!("data/associated_bounds/associated_bound_positive.rs"),
            true,
        ),
        (
            "unused_predicate_bad",
            "trait P{} trait C{type N:P;} struct S<'a>(&'a i32); impl P for S<'static>{} impl<'a>C for S<'a>{type N=Self;} fn main(){}",
            false,
        ),
        (
            "transitive_bad",
            "trait P{type A;} trait Q<T>:P<A=T>{} trait C<T>{type N:Q<T>;} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'static i32;} impl<'a>Q<&'static i32> for S<'a>{} impl<'a>C<&'a i32> for S<'a>{type N=Self;} fn main(){}",
            false,
        ),
        (
            "generic_matching",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} impl<T,U:P<A=T>> C<T> for U{type N=U;} fn main(){}",
            true,
        ),
        (
            "generic_selected",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} struct W<T>(T); impl<T>P for W<T>{type A=T;} impl<T>C<T> for W<T>{type N=Self;} fn main(){}",
            true,
        ),
        (
            "recursive_matching",
            "trait C{type N:C;} struct S; impl C for S{type N=Self;} fn main(){}",
            true,
        ),
        (
            "multiple_slots_bad",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} struct S<'a,'b>(&'a i32,&'b i32); impl<'a,'b>P for S<'a,'b>{type A=(&'a i32,&'a i32);} impl<'a,'b>C<(&'a i32,&'b i32)> for S<'a,'b>{type N=Self;} fn main(){}",
            false,
        ),
        (
            "mutual_outlives",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a:'b,'b:'a>C<&'b i32> for S<'a>{type N=Self;} fn main(){}",
            true,
        ),
        (
            "distinct_associated_value",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} struct S<'a>(&'a i32); struct W<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a>C<&'a i32> for W<'a>{type N=S<'a>;} fn main(){}",
            true,
        ),
        (
            "permuted_generic",
            "trait P{type A;} trait C<X,Y>{type N:P<A=(Y,X)>;} struct W<T,U>(T,U); impl<T,U>P for W<T,U>{type A=(U,T);} impl<T,U>C<U,T> for W<T,U>{type N=W<U,T>;} fn main(){}",
            true,
        ),
        (
            "one_way_outlives",
            "trait P{type A;} trait C<T>{type N:P<A=T>;} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a:'b,'b>C<&'b i32> for S<'a>{type N=Self;} fn main(){}",
            false,
        ),
        (
            "header_associated_bound",
            "trait Shape{type Unit:Copy;} impl<'a,S:Shape> Shape for &'a S{type Unit=S::Unit;} fn main(){}",
            true,
        ),
    ];
    let dir = std::env::temp_dir().join(format!("flux-associated-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in cases {
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            mir.diagnostics.is_empty(),
            accepts,
            "{name}: {:?}",
            mir.diagnostics
        );
        if !accepts {
            assert!(
                mir.diagnostics
                    .iter()
                    .any(|d| d.message.starts_with("associated type bound")),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nested_selected_values_remain_explicitly_unsupported() {
    // A selected P::A that itself selects Q has obligations this bound
    // proof cannot discharge. P2 owns validation of the definition itself.
    let source = "trait Q{type A;} trait P{type A;} trait C<T>{type N:P<A=T>;} struct S; impl Q for S{type A=i32;} impl P for S{type A=<S as Q>::A;} impl C<i32> for S{type N=Self;} fn main(){}";
    let dir = std::env::temp_dir().join(format!(
        "flux-associated-bound-boundary-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("boundary.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&dir)
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.iter().any(|d| d.message == "associated type bound requirements that cannot be resolved from the impl header are not supported yet"), "{:?}", mir.diagnostics);
    std::fs::remove_dir_all(dir).unwrap();
}
