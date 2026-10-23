//! Hidden inherited reference regions cannot become pointer binders.
use std::process::Command;

#[test]
fn inherited_reference_binders_reject_like_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-inherited-binders-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, declaration) in [
        (
            "static_input",
            r#"trait Base { type Out; }
trait Child: Base { fn apply(f: fn(Self::Out)); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply(f: fn(Self::Out)) {
        let x = 7;
        f(&x);
    }
}

"#,
        ),
        (
            "owner_output",
            r#"trait Base { type Out; }
trait Child: Base { fn leak(&self, f: fn() -> Self::Out) -> &'static u8; }
struct S<'a>(&'a u8);
impl<'a> Base for S<'a> { type Out = &'a u8; }
impl<'a> Child for S<'a> {
    fn leak(&self, f: fn() -> Self::Out) -> &'static u8 { f() }
}

"#,
        ),
        (
            "unsafe_input",
            r#"trait Base { type Out; }
trait Child: Base { fn apply(f: unsafe fn(Self::Out)); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply(f: unsafe fn(Self::Out)) {
        let x = 7;
        unsafe { f(&x); }
    }
}

"#,
        ),
        (
            "alias",
            r#"type Callback<T>=fn(T);
trait Base { type Out; }
trait Child: Base { fn apply(f: Callback<Self::Out>); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply(f: Callback<Self::Out>) {
        let x = 7;
        f(&x);
    }
}

"#,
        ),
        (
            "carrier",
            r#"struct Callback<T>(fn(T));
trait Base { type Out; }
trait Child: Base { fn apply(f: Callback<Self::Out>); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply(f: Callback<Self::Out>) {
        let x = 7;
        (f.0)(&x);
    }
}

"#,
        ),
        (
            "default_carrier",
            r#"struct Callback<T,U=fn(T)>{f:U,marker:std::marker::PhantomData<T>}
trait Base { type Out; }
trait Child: Base { fn apply(f: Callback<Self::Out>); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply(f: Callback<Self::Out>) {
        let x = 7;
        (f.f)(&x);
    }
}

"#,
        ),
        (
            "fn_bound",
            r#"trait Base { type Out; }
trait Child: Base { fn apply<F: Fn(Self::Out)>(f:F); }
struct S;
impl Base for S { type Out = &'static u8; }
impl Child for S {
    fn apply<F: Fn(Self::Out)>(f:F) {
        let x = 7;
        f(&x);
    }
}

"#,
        ),
        (
            "output_bound",
            r#"trait Base { type Out; }
trait Child: Base { fn leak<F:Fn()->Self::Out>(&self, f:F) -> &'static u8; }
struct S<'a>(&'a u8);
impl<'a> Base for S<'a> { type Out = &'a u8; }
impl<'a> Child for S<'a> {
    fn leak<F:Fn()->Self::Out>(&self, f:F) -> &'static u8 { f() }
}

"#,
        ),
    ] {
        let text = format!("{declaration} fn main() {{}}");
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!rustc.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message.contains("inherited associated type")
                    && d.message.contains("not supported")),
            "{name}: {:?}",
            typed.diagnostics
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
