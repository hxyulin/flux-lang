//! Supertrait edge equalities are obligations, never new assumptions.
//! Invalid sources are compiled for diagnostics only and never executed.
use std::process::Command;

#[test]
fn supertrait_equalities_match_rustc() {
    let cases = [
        (
            "assoc_wrong_region",
            include_str!("data/supertrait_equalities/assoc_wrong_region.rs"),
            false,
        ),
        (
            "assoc_wrong_type",
            include_str!("data/supertrait_equalities/assoc_wrong_type.rs"),
            false,
        ),
        (
            "assoc_escape",
            include_str!("data/supertrait_equalities/assoc_escape.rs"),
            false,
        ),
        (
            "assoc_escape_callback",
            include_str!("data/supertrait_equalities/assoc_escape_callback.rs"),
            false,
        ),
        (
            "assoc_right_region",
            include_str!("data/supertrait_equalities/assoc_right_region.rs"),
            true,
        ),
        (
            "assoc_escape_positive",
            include_str!("data/supertrait_equalities/assoc_escape_positive.rs"),
            true,
        ),
        (
            "transitive_wrong",
            "trait P{type A;} trait C<T>:P<A=T>{} trait D<T>:C<T>{} struct S; impl P for S{type A=i32;} impl<T> C<T> for S{} impl D<u8> for S{} fn main(){}",
            false,
        ),
        (
            "nested_ok",
            "trait P{type A;} trait C<T>:P<A=(T,T)>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=(&'a i32,&'a i32);} impl<'a>C<&'a i32> for S<'a>{} fn main(){}",
            true,
        ),
        (
            "nested_wrong",
            "trait P{type A;} trait C<T>:P<A=(T,T)>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=(&'a i32,&'static i32);} impl<'a>C<&'a i32> for S<'a>{} fn main(){}",
            false,
        ),
        (
            "where_wrong",
            "trait P{type A;} trait C<T> where Self:P<A=T>{} struct S; impl P for S{type A=i32;} impl C<u8> for S{} fn main(){}",
            false,
        ),
        (
            "permuted_transitive",
            "trait P{type A;} trait C<X,Y>:P<A=(Y,X)>{} trait D<X,Y>:C<Y,X>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=(&'a i32,u8);} impl<'a>C<u8,&'a i32> for S<'a>{} impl<'a>D<&'a i32,u8> for S<'a>{} fn main(){}",
            true,
        ),
        (
            "one_way_outlives",
            "trait P{type A;} trait C<T>:P<A=T>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a:'b,'b>C<&'b i32> for S<'a>{} fn main(){}",
            false,
        ),
        (
            "mutual_outlives",
            "trait P{type A;} trait C<T>:P<A=T>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a:'b,'b:'a>C<&'b i32> for S<'a>{} fn main(){}",
            true,
        ),
        (
            "header_equality",
            "trait P{type A;} trait C<T>:P<A=T>{} impl<T,U:P<A=T>> C<T> for U{} fn main(){}",
            true,
        ),
    ];
    let dir =
        std::env::temp_dir().join(format!("flux-supertrait-equalities-{}", std::process::id()));
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
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn nested_selected_projection_has_an_explicit_boundary() {
    // Normalizing the selected P::A would select another impl. Its erased
    // equality is insufficient evidence for any region requirements it has.
    let source = "trait Q{type A;} trait P{type A;} trait C<T>:P<A=T>{} struct S; impl Q for S{type A=i32;} impl P for S{type A=<S as Q>::A;} impl C<i32> for S{} fn main(){}";
    let dir = std::env::temp_dir().join(format!(
        "flux-supertrait-equality-boundary-{}",
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
    assert!(mir.diagnostics.iter().any(|d| d.message == "supertrait lifetime requirements that cannot be resolved from the impl header are not supported yet"), "{:?}", mir.diagnostics);
    std::fs::remove_dir_all(dir).unwrap();
}
