//! A trait's Self outlives bound retains concrete impl reference rows.
fn compare(name: &str, source: &str, accepted: bool) {
    let root =
        std::env::temp_dir().join(format!("flux-self-outlives-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        rustc.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    let mut errors = hir.diagnostics.clone();
    if errors.is_empty() {
        let typed = flux_mir::check_file(&hir);
        errors.extend(typed.diagnostics.clone());
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}
#[test]
fn concrete_self_bound_retains_reference_lifetimes() {
    compare(
        "native",
        include_str!("../../../tests/run/trait_self_outlives.rs"),
        true,
    );
}
#[test]
fn self_bound_does_not_prove_unrelated_relations() {
    compare(
        "stronger",
        r#"trait A{fn f<'b>(self)->&'b str where Self:'b;}impl<'a>A for &'a str{fn f<'b>(self)->&'b str where Self:'b,'b:'static{self}}fn main(){}"#,
        false,
    );
    compare(
        "unrelated",
        r#"trait A{fn f<'b>(self,other:&str)->&'b str where Self:'b;}impl<'a>A for &'a str{fn f<'b>(self,other:&str)->&'b str where Self:'b{other}}fn main(){}"#,
        false,
    );
    compare(
        "concrete_mismatch",
        r#"trait A{fn f<'b>(self)->&'b str where Self:'b;}impl<'a>A for &'a str{fn f<'b>(self)->&'static str where Self:'b{self}}fn main(){}"#,
        false,
    );
}
#[test]
fn projected_self_bound_uses_represented_output() {
    let source = r#"trait Family{type Out;}struct Marker;impl Family for Marker{type Out=String;}struct Wrap<T:Family>{value:T::Out}trait A{fn f<'b>(self)where Self:'b;}impl<T:Family>A for Wrap<T>{fn f<'b>(self)where Self:'b{}}fn main(){}"#;
    compare("projection", source, true);
    compare(
        "projection_stronger_static",
        &source.replace(
            "fn f<'b>(self)where Self:'b{}",
            "fn f<'b>(self)where Self:'b,T:'static{}",
        ),
        false,
    );
}
