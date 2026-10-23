//! Written type arguments must constrain regions, even for unused call results.
use std::process::Command;

fn check(source: &str) -> (Vec<String>, Vec<String>) {
    let (db, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(db.syntax_errors().is_empty(), "{:?}", db.syntax_errors());
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    let ty = typed
        .diagnostics
        .iter()
        .map(|d| d.message.clone())
        .collect();
    let mir = if typed.diagnostics.is_empty() {
        flux_mir::build_file(&hir, &typed)
            .diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect()
    } else {
        Vec::new()
    };
    (ty, mir)
}

#[test]
fn body_type_arguments_match_rustc() {
    let cases = [
        (
            "named",
            "fn f<'a>(x: &'a i32) -> &'a i32 { id::<&'a i32>(x) }",
            true,
        ),
        (
            "two_lifetimes",
            "fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { id::<&'a i32>(x); }",
            false,
        ),
        (
            "outlives",
            "fn f<'a, 'b: 'a>(x: &'b i32, _a: &'a i32) { id::<&'a i32>(x); }",
            true,
        ),
        (
            "wrong_direction",
            "fn f<'a: 'b, 'b>(x: &'b i32, _a: &'a i32) { id::<&'a i32>(x); }",
            false,
        ),
        (
            "return_escape",
            "fn f<'a, 'b>(x: &'a i32, _b: &'b i32) -> &'b i32 { id::<&'a i32>(x) }",
            false,
        ),
        (
            "local_escape",
            "fn f<'a>(_a: &'a i32) { let x = 1; id::<&'a i32>(&x); }",
            false,
        ),
        ("static", "fn f(x: &i32) { id::<&'static i32>(x); }", false),
        ("static_ok", "fn f() { id::<&'static i32>(&1); }", true),
        (
            "elided",
            "fn f(x: &i32) { id::<&'_ i32>(x); id::<&i32>(x); id::<_>(x); }",
            true,
        ),
        (
            "adt",
            "fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { id::<Expr<'a>>(Expr { value: x }); }",
            false,
        ),
        (
            "nested_alias",
            "fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { id::<Alias<'a>>(Nested { value: Expr { value: x } }); }",
            false,
        ),
        (
            "alias_static",
            "type Static = &'static i32; fn f(x: &i32) { id::<Static>(x); }",
            false,
        ),
        (
            "default_static",
            "struct Defaulted<T = &'static i32> { value: T } fn f(x: &i32) { id::<Defaulted>(Defaulted { value: x }); }",
            false,
        ),
        (
            "nested_ref",
            "fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { id::<(&'a i32, [Expr<'a>; 1])>((x, [Expr { value: x }])); }",
            false,
        ),
        (
            "impl_owner",
            "impl<'a> Expr<'a> { fn f<'b>(self, x: &'b i32) { id::<Expr<'a>>(Expr { value: x }); } }",
            false,
        ),
        (
            "method",
            "impl<'a> Expr<'a> { fn via<T>(&self, x: T) -> T { x } } fn f<'a, 'b>(a: Expr<'a>, x: &'b i32) { a.via::<&'a i32>(x); }",
            false,
        ),
        (
            "associated",
            "struct Factory; impl Factory { fn via<T>(x: T) -> T { x } } fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { Factory::via::<&'a i32>(x); }",
            false,
        ),
        (
            "callee_bound",
            "fn need<T: 'static>(_: T) {} fn f<'a>(x: &'a i32) { need::<&'a i32>(x); }",
            false,
        ),
        (
            "undeclared",
            "fn f(x: &i32) { id::<&'wrong i32>(x); }",
            false,
        ),
        (
            "unused_return",
            "fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { let _ = id::<&'a i32>(x); }",
            false,
        ),
        (
            "invariant",
            "fn f<'a, 'b>(x: &mut &'b i32, _a: &'a i32) { id::<&mut &'a i32>(x); }",
            false,
        ),
        (
            "function_bound_no_input",
            "fn need<T: 'static>() {} fn f<'a>(_x: &'a i32) { need::<Expr<'a>>(); }",
            false,
        ),
        (
            "well_formed",
            "fn need<T>() {} fn f<'a, T>(_x: &'a i32) { need::<&'a T>(); }",
            false,
        ),
        (
            "well_formed_ok",
            "fn need<T>() {} fn f<'a, T: 'a>(_x: &'a i32) { need::<&'a T>(); }",
            true,
        ),
        (
            "self_type",
            "fn need<T: 'static>() {} impl<'a> Expr<'a> { fn f(self) { need::<Self>(); } }",
            false,
        ),
        (
            "method_owner_type",
            "impl<T> Nested<T> { fn via<U>(&self, x: U) -> U { x } } fn f<'a, 'b>(x: &'b i32, _a: &'a i32) { Nested { value: 0 }.via::<&'a i32>(x); }",
            false,
        ),
        (
            "adt_bound",
            "struct Bound<T: 'static> { value: T } fn need<T>() {} fn f<'a>(_: &'a i32) { need::<Bound<&'a i32>>(); }",
            false,
        ),
        (
            "region_wf",
            "struct Pair<'a, 'b: 'a> { value: &'a &'b i32 } fn need<T>() {} fn f<'a, 'b>(_: &'a i32, _: &'b i32) { need::<Pair<'a, 'b>>(); }",
            false,
        ),
        (
            "closure_static",
            "fn f() { let c = || id::<&'static i32>(&1); c(); }",
            true,
        ),
    ];
    let dir = std::env::temp_dir().join(format!("flux-body-type-args-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body, accepted) in cases {
        let source = format!(
            "struct Expr<'a> {{ value: &'a i32 }}\nstruct Nested<T> {{ value: T }}\ntype Alias<'a> = Nested<Expr<'a>>;\nfn id<T>(x: T) -> T {{ x }}\n{body}\nfn main() {{}}\n"
        );
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
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (ty, mir) = check(&source);
        assert_eq!(
            ty.is_empty() && mir.is_empty(),
            accepted,
            "{name}: types {ty:?}, MIR {mir:?}"
        );
        if name == "undeclared" {
            assert!(
                ty.iter()
                    .any(|d| d.contains("undeclared lifetime name `'wrong`")),
                "{ty:?}"
            );
        }
        if !accepted && name != "undeclared" {
            assert!(ty.is_empty(), "{name} must reach borrow checking: {ty:?}");
            assert!(
                mir.iter().any(|d| d.contains("lifetime")
                    || d.contains("may not live long enough")
                    || d.contains("does not live long enough")
                    || d.contains("escapes")),
                "{name}: {mir:?}"
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn body_lifetime_boundaries_remain_closed() {
    for (body, diagnostic) in [
        (
            "fn f<'a>(x: &'a i32) { let _: &'a i32 = x; }",
            "named lifetimes in types in bodies",
        ),
        (
            "fn f<'a>(x: &'a i32) { let _ = id::<&'a i32>; }",
            "lifetimes in function item values",
        ),
        (
            "fn f<'a>(x: &'a i32) { id::<fn(&'a i32)>(f); }",
            "lifetimes in function pointer types",
        ),
        (
            "trait Ref { type Out; } impl<'a> Ref for &'a i32 { type Out = &'a i32; } fn f(x: &i32) { id::<<&'static i32 as Ref>::Out>(x); }",
            "lifetimes in associated type annotations",
        ),
        (
            "trait Ref { type Out; } impl<'a> Ref for &'a i32 { type Out = &'a i32; } type Alias = <&'static i32 as Ref>::Out; fn f(x: &i32) { id::<Alias>(x); }",
            "lifetimes in associated type annotations",
        ),
    ] {
        let source = format!("fn id<T>(x: T) -> T {{ x }} {body} fn main() {{}}");
        let (ty, mir) = check(&source);
        assert!(
            ty.iter().chain(&mir).any(|d| d.contains(diagnostic)),
            "{source}: {ty:?} {mir:?}"
        );
    }
}

#[test]
fn closure_named_arguments_remain_closed() {
    for source in [
        "fn id<T>(x: T) -> T { x } fn f<'a>(x: &'a i32) { let c = || id::<&'a i32>(x); c(); } fn main() {}",
        "fn id<T>(x: T) -> T { x } fn f<'a>(_: &'a i32) { let c = || id::<&'a i32>(&1); c(); } fn main() {}",
    ] {
        let (ty, mir) = check(source);
        assert!(
            ty.iter()
                .chain(&mir)
                .any(|d| d.contains("named type argument lifetimes in closures")),
            "{ty:?} {mir:?}"
        );
    }
}
