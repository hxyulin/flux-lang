//! Associated RHS selection must hold universally, even for unused definitions
//! whose normalized values have no region slots. Invalid cases are never run.
use std::process::Command;

#[test]
fn associated_value_selection_matches_rustc() {
    let single = "trait R{type A;} trait Q{type A;} struct S<'a>(&'a i32);";
    let pair = "trait R{type A;} trait Q{type A;} struct S<'a,'b>(&'a i32,&'b i32);";
    let cases = [
        ("self_static_bad", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=<Self as R>::A;}}"), false),
        ("explicit_static_bad", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=<S<'a> as R>::A;}}"), false),
        ("nested_actual_bad", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=<Self as R>::A;}} trait P{{}} trait C:P{{}} struct W<I>(I); impl<I:Q<A=i32>>P for W<I>{{}} impl<'a>C for W<S<'a>>{{}}"), false),
        ("nested_rhs_bad", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=([<Self as R>::A;2],);}}"), false),
        ("repeated_bad", format!("{pair} impl<'a>R for S<'a,'a>{{type A=i32;}} impl<'a,'b>Q for S<'a,'b>{{type A=<Self as R>::A;}}"), false),
        ("one_way_repeated_bad", format!("{pair} impl<'a>R for S<'a,'a>{{type A=i32;}} impl<'a:'b,'b>Q for S<'a,'b>{{type A=<Self as R>::A;}}"), false),
        ("selected_bound_static_bad", format!("{single} trait M{{}} impl M for &'static i32{{}} impl<'a>R for S<'a> where &'a i32:M{{type A=i32;}} impl<'a>Q for S<'a>{{type A=<Self as R>::A;}}"), false),
        ("bounded_bad", format!("{pair} impl<'a:'b,'b>R for S<'a,'b>{{type A=i32;}} impl<'a,'b>Q for S<'a,'b>{{type A=<Self as R>::A;}}"), false),
        ("distinct_occurrences_bad", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=(<S<'static> as R>::A,<Self as R>::A);}}"), false),
        ("alias_hidden_bad", format!("{single} impl R for S<'static>{{type A=i32;}} type Hidden<'a>=<S<'a> as R>::A; impl<'a>Q for S<'a>{{type A=Hidden<'a>;}}"), false),
        ("alias_duplicate_hidden_bad", format!("{single} impl R for S<'static>{{type A=i32;}} struct W<T>(T); type Hidden<'a>=W<<S<'a> as R>::A>; impl<'a>Q for S<'a>{{type A=(<S<'static> as R>::A,Hidden<'a>);}}"), false),
        ("self_matching", format!("{single} impl<'a>R for S<'a>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=<Self as R>::A;}}"), true),
        ("self_static_matching", format!("{single} impl R for S<'static>{{type A=i32;}} impl Q for S<'static>{{type A=<Self as R>::A;}}"), true),
        ("bounded_static_matching", format!("{single} impl R for S<'static>{{type A=i32;}} impl<'a:'static>Q for S<'a>{{type A=<Self as R>::A;}}"), true),
        ("repeated_matching", format!("{pair} impl<'a>R for S<'a,'a>{{type A=i32;}} impl<'a>Q for S<'a,'a>{{type A=<Self as R>::A;}}"), true),
        ("bounded_matching", format!("{pair} impl<'a:'b,'b>R for S<'a,'b>{{type A=i32;}} impl<'a:'b,'b>Q for S<'a,'b>{{type A=<Self as R>::A;}}"), true),
        ("mutual_matching", format!("{pair} impl<'a>R for S<'a,'a>{{type A=i32;}} impl<'a:'b,'b:'a>Q for S<'a,'b>{{type A=<Self as R>::A;}}"), true),
        ("nested_rhs_matching", format!("{single} impl<'a>R for S<'a>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=([<Self as R>::A;2],);}}"), true),
        ("duplicate_matching", format!("{single} impl<'a>R for S<'a>{{type A=i32;}} impl<'a>Q for S<'a>{{type A=(<Self as R>::A,<Self as R>::A);}}"), true),
        ("generic_header", "trait R{type A;} trait Q{type A;} struct W<T>(T); impl<T:R>Q for W<T>{type A=T::A;}".into(), true),
        ("generic_selected", "trait R{type A;} trait Q{type A;} trait M{} struct W<T>(T); impl<T:M>R for W<T>{type A=i32;} impl<T:M>Q for W<T>{type A=<Self as R>::A;}".into(), true),
        ("alias_regionfree", "trait R{type A;} trait Q{type A;} struct S; impl R for S{type A=i32;} type Projected=<S as R>::A; impl Q for S{type A=Projected;}".into(), true),
        ("nested_projection", "trait R{type A;} trait Q{type A;} trait N{type A;} struct S; impl R for S{type A=S;} impl Q for S{type A=<<Self as R>::A as R>::A;} impl N for S{type A=<Self as Q>::A;}".into(), true),
    ];
    let dir = std::env::temp_dir().join(format!(
        "flux-associated-value-regions-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in cases {
        let source = format!("{source} fn main(){{}}");
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
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
        let (db, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        if name == "alias_hidden_bad" {
            // Lifetime arguments on a type alias that resolves directly to a
            // projection already hit the guarded annotation boundary.
            assert!(!typed.diagnostics.is_empty(), "{name}: expected rejection");
            continue;
        }
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
                    .any(|d| d.message.starts_with("associated type projection")),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
