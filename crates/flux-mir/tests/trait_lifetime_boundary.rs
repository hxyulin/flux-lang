//! Trait lifetime metadata is a prerequisite, not permission to accept erased
//! obligations. Keep the boundary closed until region-aware selection lands.
use std::process::Command;

#[test]
fn trait_lifetime_obligations_remain_closed_against_rustc_oracles() {
    let cases = [
        (
            "ast_node",
            include_str!("../../flux-ty/tests/data/trait_lifetime_regions/ast_node.rs"),
            true,
            "",
        ),
        (
            "static_impl",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T: Tr<'a>>(_: T, _: &'a i32) {} static VALUE: i32 = 3; fn main() { need(S, &VALUE); }",
            true,
            "",
        ),
        (
            "nonstatic_obligation",
            "trait Tr<'a> {} struct S; impl Tr<'static> for S {} fn need<'a,T: Tr<'a>>(_: T, _: &'a i32) {} fn bad<'a>(x: &'a i32) { need(S, x); } fn main() {}",
            false,
            "escapes",
        ),
        (
            "wrong_input",
            "trait Tr<'a> { fn get(x: &'a i32) -> &'a i32; } struct S; impl<'a> Tr<'a> for S { fn get(x: &'static i32) -> &'a i32 { x } } fn main() {}",
            false,
            "not compatible",
        ),
        (
            "bad_output",
            "trait Tr<'a> { fn get(x: &'a i32) -> &'a i32; } struct S; impl<'a> Tr<'a> for S { fn get(x: &'a i32) -> &'static i32 { x } } fn main() {}",
            false,
            "lifetime may not live long enough",
        ),
        (
            "generic_escape",
            "trait Tr<'a> { fn get(self) -> &'a i32; } fn bad<'a,N:Tr<'a>>(node:N) -> &'static i32 { node.get() } fn main() {}",
            false,
            "lifetime may not live long enough",
        ),
        (
            "repeated_impl",
            "trait Tr<'a,'b> {} struct S; impl<'a> Tr<'a,'a> for S {} fn need<'a,'b,T:Tr<'a,'b>>(_:T,_:&'a mut &'a i32,_:&'b mut &'b i32) {} fn bad<'a,'b>(x:&'a mut &'a i32,y:&'b mut &'b i32) { need(S,x,y); } fn main() {}",
            false,
            "lifetime may not live long enough",
        ),
        (
            "bounded_impl",
            "trait Tr<'a> {} struct Node<'a>(&'a i32); impl<'a:'static> Tr<'a> for Node<'a> {} fn need<'a,T:Tr<'a>>(_:T,_:&'a i32) {} fn bad<'a>(x:&'a i32) { need(Node(x),x); } fn main() {}",
            false,
            "escapes",
        ),
        (
            "overlapping_impls",
            "trait Tr<'a> {} struct S; impl<'a> Tr<'a> for S {} impl Tr<'static> for S {} fn main() {}",
            false,
            "conflicting implementations",
        ),
        (
            "distinct_lifetime_impls",
            "trait Tr<'a> {} struct Node<'a>(&'a i32); impl<'a> Tr<'a> for Node<'a> {} impl<'a> Tr<'static> for Node<'a> {} fn main() {}",
            false,
            "conflicting implementations",
        ),
        (
            "undeclared_trait_lifetime",
            "trait Tr<'a> { fn get(x:&'b i32); } fn main() {}",
            false,
            "undeclared lifetime",
        ),
    ];
    let dir = std::env::temp_dir().join(format!(
        "flux-trait-lifetime-boundary-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, rustc_accepts, rustc_message) in cases {
        let path = dir.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&dir)
            .arg(&path)
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.success(), rustc_accepts, "{name}: {stderr}");
        assert!(stderr.contains(rustc_message), "{name}: {stderr}");
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed
                .diagnostics
                .iter()
                .any(|d| d.message == "lifetime parameters on traits are not supported yet"),
            "{name}: {:?}",
            typed.diagnostics
        );
        // Error recovery must remain robust through MIR construction. The
        // signature diagnostic above is what prevents code generation.
        flux_mir::build_file(&hir, &typed);
    }
    std::fs::remove_dir_all(dir).unwrap();
}
