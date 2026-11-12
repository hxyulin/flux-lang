//! Defaulted Self arguments retain their occurrence's lifetime rows.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-default-rhs-{name}-{}", std::process::id()));
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
fn defaulted_rhs_uses_qualified_self_lifetimes() {
    compare(
        "default",
        include_str!("../../../tests/run/trait_default_rhs.rs"),
        true,
    );
}
#[test]
fn explicit_rhs_lifetime_remains_distinct() {
    let source = r#"trait Pick<Rhs=Self>{type Out;fn pick(self,rhs:Rhs)->Self::Out;}
impl<'a,'b> Pick<&'b u8> for &'a u8{type Out=&'b u8;fn pick(self,rhs:&'b u8)->Self::Out{rhs}}
struct W<T>(T);
impl<'a,'b,T> Pick<&'b T> for &'a W<T> where &'a T:Pick<&'b T>{type Out=<&'a T as Pick<&'b T>>::Out;fn pick(self,rhs:&'b T)->Self::Out{(&self.0).pick(rhs)}}
fn bad<'a,'b>(left:&'a W<u8>,right:&'b u8)->&'a u8{left.pick(right)}fn main(){}"#;
    compare("distinct", source, false);
    compare(
        "outlives",
        &source.replace("fn bad<'a,'b>", "fn bad<'a,'b:'a>"),
        true,
    );
}
