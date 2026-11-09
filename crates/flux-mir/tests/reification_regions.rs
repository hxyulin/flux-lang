//! Immediate function item reification retains the written generic regions.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-reification-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    let source = format!("{source}\nfn main() {{}}\n");
    std::fs::write(&path, &source).unwrap();
    let reference = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        reference.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, &source);
    let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
        if errors.is_empty() {
            errors.extend(
                flux_mir::build_file(&hir, &typed)
                    .diagnostics
                    .into_iter()
                    .map(|d| d.message),
            );
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}

#[test]
fn static_argument() {
    compare(
        "static_argument",
        r#"fn require<T: 'static>() {} fn f() { let _: fn() = require::<&'static u32>; }"#,
        true,
    );
}

#[test]
fn hidden_argument_cannot_be_static() {
    compare(
        "hidden_argument_cannot_be_static",
        r#"fn require<T: 'static>() {} fn f<'a>() { let _: fn() = require::<&'a u32>; }"#,
        false,
    );
}

#[test]
fn hidden_nested_argument_cannot_be_static() {
    compare(
        "hidden_nested_argument_cannot_be_static",
        r#"fn require<T: 'static>() {} fn f<'a>() { let _: fn() = require::<Vec<&'a u32>>; }"#,
        false,
    );
}

#[test]
fn named_identity() {
    compare(
        "named_identity",
        r#"fn id<T>(x:T)->T{x} fn f<'a>(x:&'a u32)->&'a u32 { let p: fn(&'a u32)->&'a u32 = id::<&'a u32>; p(x) }"#,
        true,
    );
}

#[test]
fn static_object_argument() {
    compare(
        "static_object_argument",
        r#"fn require<T: 'static>() {} fn f() { let _: fn() = require::<Box<dyn std::error::Error>>; }"#,
        true,
    );
}

#[test]
fn named_object_cannot_be_static() {
    compare(
        "named_object_cannot_be_static",
        r#"fn require<T: 'static>() {} fn f<'a>() { let _: fn() = require::<Box<dyn std::error::Error + 'a>>; }"#,
        false,
    );
}

#[test]
fn fixed_argument_cannot_be_universal() {
    compare(
        "fixed_argument_cannot_be_universal",
        r#"fn id<T>(x:T)->T{x} fn f<'a>() { let _: for<'b> fn(&'b u32)->&'b u32 = id::<&'a u32>; }"#,
        false,
    );
}

#[test]
fn invariant_identity() {
    compare(
        "invariant_identity",
        r#"fn id<T>(x:T)->T{x} fn f<'a>(x:std::cell::Cell<&'a u32>)->std::cell::Cell<&'a u32> { let p: fn(std::cell::Cell<&'a u32>)->std::cell::Cell<&'a u32> = id::<std::cell::Cell<&'a u32>>; p(x) }"#,
        true,
    );
}

#[test]
fn output_cannot_escape() {
    compare(
        "output_cannot_escape",
        r#"fn id<T>(x:T)->T{x} fn f<'a>() { let _: fn(&'static u32)->&'static u32 = id::<&'a u32>; }"#,
        false,
    );
}

#[test]
fn closure_named_identity() {
    compare(
        "closure_named_identity",
        r#"fn id<T>(x:T)->T{x} fn apply<'a>(f:fn(&'a u32)->&'a u32,x:&'a u32)->&'a u32{f(x)} fn f<'a>(x:&'a u32)->&'a u32 { (|| apply(id::<&'a u32>,x))() }"#,
        true,
    );
}

#[test]
fn closure_hidden_bound() {
    compare(
        "closure_hidden_bound",
        r#"fn require<T:'static>(){} fn f<'a>() { (|| {let _:fn()=require::<&'a u32>;})(); }"#,
        false,
    );
}

#[test]
fn projection_argument() {
    compare(
        "projection_argument",
        r#"trait Family {type Item;} struct S;impl Family for S {type Item=&'static u32;} fn require<T:'static>(){} fn f(){let _:fn()=require::<<S as Family>::Item>;}"#,
        true,
    );
}

#[test]
fn promoted_table() {
    compare(
        "promoted_table",
        r#"struct Table {f:fn()->usize} fn size<T:'static>()->usize{std::mem::size_of::<T>()} fn f()->&'static Table {&Table {f:size::<Box<dyn std::error::Error>>}}"#,
        true,
    );
}

#[test]
fn promoted_hidden_bound() {
    compare(
        "promoted_hidden_bound",
        r#"struct Table {f:fn()->usize} fn size<T:'static>()->usize{std::mem::size_of::<T>()} fn f<'a>()->&'static Table {&Table {f:size::<&'a u32>}}"#,
        false,
    );
}

#[test]
fn cast_static_argument() {
    compare(
        "cast_static_argument",
        r#"fn require<T:'static>(){} fn f(){ let _=require::<&'static u32> as fn(); }"#,
        true,
    );
}

#[test]
fn cast_hidden_argument() {
    compare(
        "cast_hidden_argument",
        r#"fn require<T:'static>(){} fn f<'a>(){ let _=require::<&'a u32> as fn(); }"#,
        false,
    );
}

#[test]
fn cast_fixed_argument_cannot_be_universal() {
    compare(
        "cast_fixed_argument_cannot_be_universal",
        r#"fn id<T>(x:T)->T{x} fn f<'a>(){let _=id::<&'a u32> as for<'b> fn(&'b u32)->&'b u32;}"#,
        false,
    );
}

#[test]
fn unwritten_argument_may_be_inferred_static() {
    compare(
        "unwritten_argument_may_be_inferred_static",
        r#"fn require<T:'static>(){} fn f(){ let _:fn()=require::<&u32>; }"#,
        true,
    );
}

#[test]
fn address_hidden_argument() {
    compare(
        "address_hidden_argument",
        r#"fn require<T:'static>(){} fn f<'a>(){ let _=require::<&'a u32> as usize; }"#,
        false,
    );
}

#[test]
fn raw_address_hidden_argument() {
    compare(
        "raw_address_hidden_argument",
        r#"fn require<T:'static>(){} fn f<'a>(){ let _=require::<&'a u32> as *const (); }"#,
        false,
    );
}

#[test]
fn address_static_argument() {
    compare(
        "address_static_argument",
        r#"fn require<T:'static>(){} fn f(){ let _=require::<&'static u32> as usize; }"#,
        true,
    );
}
