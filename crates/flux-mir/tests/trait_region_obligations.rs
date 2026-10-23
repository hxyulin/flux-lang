//! Exercise actual MIR constraints while the public declaration gate stays closed.
use std::process::Command;

#[test]
fn impl_header_regions_constrain_obligations_like_rustc() {
    let cases = [
        (
            "impl_owner_bound",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} struct Wrap<T>(T); impl<'a,T:Tr<'a>> Wrap<T> { fn need(self,_:&'a i32) {} } fn f<'a>(x:&'a i32) {Wrap(S).need(x);}",
            false,
        ),
        (
            "static_ok",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a i32) {} fn f(x:&'static i32) {need(S,x);}",
            true,
        ),
        (
            "static_unused",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
        (
            "repeated_ok",
            "trait Tr<'a,'b> {} struct S; impl<'x> Tr<'x,'x> for S {} fn need<'a,'b,T:Tr<'a,'b>>(_:T,_:&'a mut &'a i32,_:&'b mut &'b i32) {} fn f<'a>(x:&'a mut &'a i32,y:&'a mut &'a i32) {need(S,x,y);}",
            true,
        ),
        (
            "repeated_bad",
            "trait Tr<'a,'b> {} struct S; impl<'x> Tr<'x,'x> for S {} fn need<'a,'b,T:Tr<'a,'b>>(_:T,_:&'a mut &'a i32,_:&'b mut &'b i32) {} fn f<'a,'b>(x:&'a mut &'a i32,y:&'b mut &'b i32) {need(S,x,y);}",
            false,
        ),
        (
            "bounded_ok",
            "trait Tr<'a> {} struct S; impl<'a:'b,'b> Tr<'a> for &'b S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a:'b,'b>(s:&'b S,x:&'a mut &'a i32) {need::<&'b S>(s,x);}",
            true,
        ),
        (
            "bounded_bad",
            "trait Tr<'a> {} struct S; impl<'a:'b,'b> Tr<'a> for &'b S {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b>(s:&'b S,x:&'a mut &'a i32) {need::<&'b S>(s,x);}",
            false,
        ),
        (
            "self_header_equality",
            "trait Tr<'a> {} struct Node<'a>(&'a i32); impl<'a> Tr<'a> for Node<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a mut &'a i32) {} fn f<'a,'b>(n:Node<'b>,x:&'a mut &'a i32) {need(n,x);}",
            false,
        ),
        (
            "trait_type_argument",
            "trait Tr<T> {} struct S; impl Tr<&'static i32> for S {} fn need<'a,T:Tr<&'a i32>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
        (
            "recursive_bound",
            "trait Tr<'a> {} trait Outer<'a> {} struct S; impl Tr<'static> for S {} impl<'a,T:Tr<'a>> Outer<'a> for T {} fn need<'a,T:Outer<'a>>(_:T,_:&'a i32) {} fn f<'a>(x:&'a i32) {need(S,x);}",
            false,
        ),
    ];
    let dir = std::env::temp_dir().join(format!(
        "flux-trait-region-obligations-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in cases {
        let source = format!("{source} fn main() {{}}");
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
        assert!(
            typed.diagnostics.iter().all(|d| d.message
                == "lifetime parameters on traits are not supported yet"
                || d.message.starts_with("trait takes 0 lifetime arguments")
                || d.message
                    == "lifetimes in the generic arguments of bounds are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
        if name != "trait_type_argument" {
            assert!(
                typed
                    .diagnostics
                    .iter()
                    .any(|d| d.message == "lifetime parameters on traits are not supported yet"),
                "declaration guard must remain closed"
            );
        }
        // Keep the declaration diagnostics intact: only inspect the independent
        // borrow-checking result, never treating this as public acceptance.
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
                    .any(|d| d.message.contains("lifetime") || d.message.contains("escapes")),
                "{name}: {:?}",
                mir.diagnostics
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
