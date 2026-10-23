//! Selected supertrait impl equalities must hold even without region slots.
//! Invalid programs are compiled only, never executed.
use std::process::Command;

#[test]
fn selected_impl_projection_types_match_rustc() {
    let cases = [
        (
            "fn_wrong_output",
            "trait P{} trait C:P{} struct W<F>(F); impl<F:Fn(i32)->i32>P for W<F>{} impl<F:Fn(i32)->u8>C for W<F>{} fn main(){}",
            false,
        ),
        (
            "fn_once_wrong_output",
            "trait P{} trait C:P{} struct W<F>(F); impl<F:FnOnce(i32)->i32>P for W<F>{} impl<F:Fn(i32)->u8>C for W<F>{} fn main(){}",
            false,
        ),
        (
            "iterator_wrong_item",
            "trait P{} trait C:P{} struct W<I>(I); impl<I:Iterator<Item=i32>>P for W<I>{} impl<I:Iterator<Item=u8>>C for W<I>{} fn main(){}",
            false,
        ),
        (
            "same_slot_count_wrong_type",
            "trait P{} trait C:P{} struct W<I>(I); impl<'a,I:Iterator<Item=&'a i32>>P for W<I>{} impl<'a,I:Iterator<Item=&'a u8>>C for W<I>{} fn main(){}",
            false,
        ),
        (
            "fn_matching",
            "trait P{} trait C:P{} struct W<F>(F); impl<F:Fn(i32)->i32>P for W<F>{} impl<F:Fn(i32)->i32>C for W<F>{} fn main(){}",
            true,
        ),
        (
            "fn_once_matching",
            "trait P{} trait C:P{} struct W<F>(F); impl<F:FnOnce(i32)->i32>P for W<F>{} impl<F:Fn(i32)->i32>C for W<F>{} fn main(){}",
            true,
        ),
        (
            "iterator_matching",
            "trait P{} trait C:P{} struct W<I>(I); impl<I:Iterator<Item=i32>>P for W<I>{} impl<I:Iterator<Item=i32>>C for W<I>{} fn main(){}",
            true,
        ),
        (
            "unrelated_higher_ranked",
            "trait P{} trait C:P{} struct W<F>(F); impl<F>P for W<F>{} impl<F:Fn(&i32)->i32>C for W<F>{} fn main(){}",
            true,
        ),
        (
            "runtime",
            include_str!("../../../tests/run/impl_supertrait_projection_types.rs"),
            true,
        ),
    ];
    let dir = std::env::temp_dir().join(format!(
        "flux-supertrait-projection-types-{}",
        std::process::id()
    ));
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
            assert!(mir.diagnostics.iter().any(|d| d.message == "supertrait lifetime requirements that cannot be resolved from the impl header are not supported yet"), "{name}: {:?}", mir.diagnostics);
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
