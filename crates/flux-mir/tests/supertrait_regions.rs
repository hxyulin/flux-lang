//! Declaration-time supertrait checking, compared with rustc. Invalid
//! programs are only checked; their escape functions are never executed.
use std::process::Command;

#[test]
fn impl_supertrait_regions_match_rustc() {
    let cases = [
        (
            "empty",
            include_str!("data/supertrait_regions/negative_enabled_supertrait.rs"),
            false,
        ),
        (
            "escape",
            include_str!("data/supertrait_regions/negative_enabled_supertrait_escape.rs"),
            false,
        ),
        (
            "callback_escape",
            "trait P {fn read(self)->&'static i32;} trait C:P {} struct S<'a>(&'a i32); impl P for S<'static>{fn read(self)->&'static i32{self.0}} impl<'a>C for S<'a>{} fn read<T:C>(x:T)->&'static i32{x.read()} fn call<T:C,F:FnOnce(T)->i32>(x:T,f:F)->i32{f(x)} fn escape<'a>(x:&'a i32)->i32{call(S(x),|v| *read(v))} fn main(){}",
            false,
        ),
        (
            "owner_escape",
            "trait P {fn read(self)->&'static i32;} trait C:P {} struct S<'a>(&'a i32); struct W<T>(T); impl P for S<'static>{fn read(self)->&'static i32{self.0}} impl<T:P>P for W<T>{fn read(self)->&'static i32{self.0.read()}} impl<'a>C for W<S<'a>>{} fn read<T:C>(x:T)->&'static i32{x.read()} fn escape<'a>(x:&'a i32)->&'static i32{read(W(S(x)))} fn main(){}",
            false,
        ),
        (
            "transitive",
            "trait P {} trait C:P {} trait D:C {} struct S<'a>(&'a i32); impl P for S<'static>{} impl C for S<'static>{} impl<'a>D for S<'a>{} fn main(){}",
            false,
        ),
        (
            "wrapper",
            "trait P {} trait C:P {} struct S<'a>(&'a i32); struct W<T>(T); impl P for S<'static>{} impl<T:P>P for W<T>{} impl<'a>C for W<S<'a>>{} fn main(){}",
            false,
        ),
        (
            "bounded",
            "trait P {} trait C:P {} struct S<'a,'b>(&'a i32,&'b i32); impl<'a,'b:'a>P for S<'a,'b>{} impl<'a,'b>C for S<'a,'b>{} fn main(){}",
            false,
        ),
        (
            "repeated",
            "trait P {} trait C:P {} struct S<'a,'b>(&'a i32,&'b i32); impl<'a>P for S<'a,'a>{} impl<'a,'b>C for S<'a,'b>{} fn main(){}",
            false,
        ),
        (
            "type_bound",
            "trait P {} trait C:P {} struct W<T>(T); impl<T:'static>P for W<T>{} impl<T>C for W<T>{} fn main(){}",
            false,
        ),
        (
            "trait_argument",
            "trait P<T>{} trait C<T>:P<T>{} struct S<'a>(&'a i32); impl P<&'static i32> for S<'static>{} impl<'a>C<&'a i32> for S<'a>{} fn main(){}",
            false,
        ),
        (
            "uniform",
            "trait P {} trait C:P {} struct S<'a>(&'a i32); impl<'a>P for S<'a>{} impl<'a>C for S<'a>{} fn main(){}",
            true,
        ),
        (
            "static",
            "trait P {} trait C:P {} struct S<'a>(&'a i32); impl P for S<'static>{} impl C for S<'static>{} fn main(){}",
            true,
        ),
        (
            "static_bound",
            "trait P {} trait C:P {} struct S<'a>(&'a i32); impl P for S<'static>{} impl<'a:'static>C for S<'a>{} fn main(){}",
            true,
        ),
        (
            "bounded_ok",
            "trait P {} trait C:P {} struct S<'a,'b>(&'a i32,&'b i32); impl<'a,'b:'a>P for S<'a,'b>{} impl<'a,'b:'a>C for S<'a,'b>{} fn main(){}",
            true,
        ),
        (
            "repeated_ok",
            "trait P {} trait C:P {} struct S<'a,'b>(&'a i32,&'b i32); impl<'a>P for S<'a,'a>{} impl<'a>C for S<'a,'a>{} fn main(){}",
            true,
        ),
        (
            "wrapper_ok",
            "trait P {} trait C:P {} struct W<T>(T); impl<T:P>P for W<T>{} impl<T:P>C for W<T>{} fn main(){}",
            true,
        ),
        (
            "type_bound_ok",
            "trait P {} trait C:P {} struct W<T>(T); impl<T:'static>P for W<T>{} impl<T:'static>C for W<T>{} fn main(){}",
            true,
        ),
        (
            "trait_argument_ok",
            "trait P<T>{} trait C<T>:P<T>{} struct S<'a>(&'a i32); impl<'a>P<&'a i32> for S<'a>{} impl<'a>C<&'a i32> for S<'a>{} fn main(){}",
            true,
        ),
        (
            "bound_super",
            "trait P {} trait C:P {} struct W<T>(T); impl<T:P>P for W<T>{} impl<T:C>C for W<T>{} fn main(){}",
            true,
        ),
        (
            "callback_bound",
            "trait P {} trait C:P {} struct W<F>(F); impl<F:Fn(i32)->i32>P for W<F>{} impl<F:Fn(i32)->i32>C for W<F>{} fn main(){}",
            true,
        ),
        (
            "unrelated_callback",
            "trait P {} trait C:P {} struct W<F>(F); impl<F>P for W<F>{} impl<F:Fn(&i32)->i32>C for W<F>{} fn main(){}",
            true,
        ),
        (
            "projection_escape",
            "trait P {} trait C:P {} struct W<I>(I); impl<I:Iterator<Item=&'static i32>>P for W<I>{} impl<'a,I:Iterator<Item=&'a i32>>C for W<I>{} fn main(){}",
            false,
        ),
        (
            "projection_ok",
            "trait P {} trait C:P {} struct W<I>(I); impl<'a,I:Iterator<Item=&'a i32>>P for W<I>{} impl<'a,I:Iterator<Item=&'a i32>>C for W<I>{} fn main(){}",
            true,
        ),
        (
            "runtime",
            include_str!("../../../tests/run/impl_supertrait_regions.rs"),
            true,
        ),
    ];
    let dir = std::env::temp_dir().join(format!("flux-supertrait-regions-{}", std::process::id()));
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
                mir.diagnostics.iter().any(|d| d.message
                    == "supertrait lifetime requirements are not satisfied by the impl header"),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unrepresented_supertrait_regions_have_an_explicit_boundary() {
    // Trait headers have no region tables for written supertrait arguments.
    // Higher-ranked callbacks needed by a selected supertrait impl also need
    // binder-aware entailment; unrelated callback bounds remain accepted.
    let sources = [
        "trait P<T>{} trait C:P<&'static i32>{} struct S; impl P<&'static i32> for S{} impl C for S{} fn main(){}",
        "trait P{} trait C:P{} struct W<F>(F); impl<F:Fn(&i32)->i32>P for W<F>{} impl<F:Fn(&i32)->i32>C for W<F>{} fn main(){}",
    ];
    let dir = std::env::temp_dir().join(format!("flux-supertrait-boundary-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (i, source) in sources.iter().enumerate() {
        let path = dir.join(format!("boundary_{i}.rs"));
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
        assert!(hir.diagnostics.is_empty());
        let typed = flux_mir::check_file(&hir);
        if i == 0 {
            assert!(typed.diagnostics.iter().any(|d| d.message
                == "lifetimes in the generic arguments of bounds are not supported yet"));
            continue;
        }
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.iter().any(|d| d.message == "supertrait lifetime requirements that cannot be resolved from the impl header are not supported yet"), "{:?}", mir.diagnostics);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
