//! Selection lifetimes need no output rows when a projection is closed and region-free.
fn check(source: &str, accepted: bool) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "flux-closed-projection-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let reference = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        reference.status.success(),
        accepted,
        "{}",
        String::from_utf8_lossy(&reference.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    let mut errors = typed.diagnostics.clone();
    if errors.is_empty() {
        errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
    }
    assert_eq!(errors.is_empty(), accepted, "{errors:?}");
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn concrete_output_does_not_store_selection_lifetimes() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=u32;}fn f<'a>(_: &'a u8)->u32{let x:<S as Select<'a>>::Out=7;x}fn main(){assert_eq!(f(&0),7);}",
        true,
    );
}
#[test]
fn borrowed_output_cannot_escape() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=&'a u8;}fn f<'a>(x:&'a u8)->&'static u8{let y:<S as Select<'a>>::Out=x;y}fn main(){}",
        false,
    );
}
#[test]
fn selection_lifetime_still_must_be_declared() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a>Select<'a> for S{type Out=u32;}fn f(){let _: <S as Select<'missing>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn closed_output_does_not_prove_selection_lifetime_obligations() {
    check(
        "trait Select<'a>{type Out;}struct S;impl Select<'static> for S{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S as Select<'a>>::Out=7;}fn main(){}",
        false,
    );
}

#[test]
fn repeated_owner_lifetimes_match_the_impl_header() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S<'a> as Select<'a>>::Out=7;}fn main(){}",
        true,
    );
}
#[test]
fn repeated_owner_lifetimes_cannot_be_independent() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a,'b>(_: &'a u8,_: &'b u8){let _: <S<'a> as Select<'b>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn static_owner_lifetime_matches_a_static_impl() {
    check(
        "trait Select<'a>{type Out;}struct S;impl Select<'static> for S{type Out=u32;}fn f(){let _: <S as Select<'static>>::Out=7;}fn main(){}",
        true,
    );
}
#[test]
fn selected_impl_lifetime_bounds_are_not_erased() {
    check(
        "trait Select<'a>{type Out;}struct S;impl<'a:'static>Select<'a> for S{type Out=u32;}fn f<'a>(_: &'a u8){let _: <S as Select<'a>>::Out=7;}fn main(){}",
        false,
    );
}
#[test]
fn owner_and_self_lifetimes_must_match_together() {
    check(
        "trait Select<'a>{type Out;}struct S<'a>(&'a u8);impl<'a>Select<'a> for S<'a>{type Out=u32;}fn f<'a,'b>(_: &'a u8,_: &'b u8){let _: <S<'a> as Select<'b>>::Out=7;}fn main(){}",
        false,
    );
}

#[test]
fn deferred_annotations_keep_local_type_and_alias_scopes() {
    check(
        include_str!("../../../tests/run/local_type_annotation_scopes.rs"),
        true,
    );
}
#[test]
fn local_alias_cannot_extend_a_borrow() {
    check(
        "fn bad<'a>(input:&'a u8)->&'static u8{struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let value:Alias<'static>=Hold(input);value.0}fn main(){}",
        false,
    );
}
#[test]
fn shadowed_alias_cannot_erase_its_static_annotation() {
    check(
        "fn bad<'a>(input:&'a u8){struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let _:Alias<'a>=Hold(input);{struct Hold<'b>(&'b u8);type Alias<'b>=Hold<'b>;let _:Alias<'static>=Hold(input);}}fn main(){}",
        false,
    );
}

#[test]
fn closed_nested_outputs_and_type_only_families_keep_their_selection_proofs() {
    check(
        "trait F{type Out;}struct S;impl F for S{type Out=<u8 as std::ops::Add<u8>>::Output;}fn main(){let x:<S as F>::Out=7;assert_eq!(x,7);}",
        true,
    );
    check(
        "trait F{type Out<T:Copy>:Copy;}struct S;impl F for S{type Out<U:Copy>=U;}fn main(){let x:<S as F>::Out<u32>=7;assert_eq!(x,7);}",
        true,
    );
    check(
        "trait F{type Out<T:Copy>;}struct S;impl F for S{type Out<U:Copy>=u32;}fn main(){let _:<S as F>::Out<String>=7;}",
        false,
    );
}
