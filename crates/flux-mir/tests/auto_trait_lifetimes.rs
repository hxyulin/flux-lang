//! Explicit auto-trait impls retain their header lifetimes at every use.
use std::process::Command;

#[test]
fn auto_trait_lifetimes_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-auto-lifetimes-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, accepted, declaration, usage) in [
        (
            "repeated_type_short",
            false,
            "struct W<A,B>(A,B); unsafe impl<T> Send for W<T,T> {}",
            "fn test<'a>() { send::<W<&'a u8, &'static u8>>(); }",
        ),
        (
            "opaque_repeated_type_short",
            true,
            "struct W<A,B>(A,B); unsafe impl<T> Send for W<T,T> {} static VALUE: u8 = 1;",
            "fn test<'a>(value: &'a u8) -> impl Send + 'a { W::<&'a u8, &'static u8>(value, &VALUE) }",
        ),
        (
            "opaque_invariant_repeated_type_short",
            false,
            "use std::cell::Cell; struct W<A,B>(Cell<A>,Cell<B>); unsafe impl<T> Send for W<T,T> {} static VALUE: u8 = 1;",
            "fn test<'a>(value: &'a u8) -> impl Send + 'a { W::<&'a u8, &'static u8>(Cell::new(value), Cell::new(&VALUE)) }",
        ),
        (
            "object_static",
            true,
            "trait Marker {} struct W<T: ?Sized>(T); unsafe impl Send for W<dyn Marker> {}",
            "fn test() { send::<W<dyn Marker>>(); }",
        ),
        (
            "object_short",
            false,
            "trait Marker {} struct W<T: ?Sized>(T); unsafe impl Send for W<dyn Marker> {}",
            "fn test<'a>() { send::<W<dyn Marker + 'a>>(); }",
        ),
        (
            "reference_static",
            true,
            "struct W<'a>(&'a u8); unsafe impl Send for W<'static> {}",
            "fn test() { send::<W<'static>>(); }",
        ),
        (
            "reference_short",
            false,
            "struct W<'a>(&'a u8); unsafe impl Send for W<'static> {}",
            "fn test<'a>() { send::<W<'a>>(); }",
        ),
        (
            "repeated_equal",
            true,
            "struct W<'a,'b>(&'a u8,&'b u8); unsafe impl<'a> Send for W<'a,'a> {}",
            "fn test<'a>() { send::<W<'a,'a>>(); }",
        ),
        (
            "repeated_different",
            false,
            "struct W<'a,'b>(&'a u8,&'b u8); unsafe impl<'a> Send for W<'a,'a> {}",
            "fn test<'a>() { send::<W<'a,'static>>(); }",
        ),
        (
            "outlives_valid",
            true,
            "struct W<'a,'b>(&'a u8,&'b u8); unsafe impl<'a:'b,'b> Send for W<'a,'b> {}",
            "fn test<'a,'b>() where 'a:'b { send::<W<'a,'b>>(); }",
        ),
        (
            "outlives_missing",
            false,
            "struct W<'a,'b>(&'a u8,&'b u8); unsafe impl<'a:'b,'b> Send for W<'a,'b> {}",
            "fn test<'a,'b>() { send::<W<'a,'b>>(); }",
        ),
        (
            "opaque_object_short",
            false,
            "trait Marker {} struct W<T: ?Sized>(Box<T>); unsafe impl Send for W<dyn Marker> {}",
            "fn test<'a>(value: Box<dyn Marker + 'a>) -> impl Send + 'a { W(value) }",
        ),
        (
            "method_object_short",
            false,
            "trait Marker {} struct W<T: ?Sized>(T); unsafe impl Send for W<dyn Marker> {}",
            "trait Tr { fn run<'a>(); } impl Tr for () { fn run<'a>() where W<dyn Marker + 'a>: Send {} }",
        ),
        (
            "sync_object_short",
            false,
            "trait Marker {} struct W<T: ?Sized>(T); unsafe impl Sync for W<dyn Marker> {}",
            "fn sync<T: Sync + ?Sized>() {} fn test<'a>() { sync::<W<dyn Marker + 'a>>(); }",
        ),
        (
            "nested_object_short",
            false,
            "trait Marker {} struct W<T: ?Sized>(T); struct Outer<T>(T); unsafe impl Send for W<dyn Marker> {}",
            "fn test<'a>() { send::<Outer<Box<W<dyn Marker + 'a>>>>(); }",
        ),
    ] {
        let source =
            format!("{declaration}\nfn send<T: Send + ?Sized>() {{}}\n{usage}\nfn main() {{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let oracle = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let diagnostics = if typed.diagnostics.is_empty() {
            flux_mir::build_file(&hir, &typed).diagnostics
        } else {
            typed.diagnostics
        };
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "Flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(root).unwrap();
}
