//! Opaque parser adapters preserve independently borrowed result rows.
use std::process::Command;

#[test]
fn opaque_parser_result_regions_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-opaque-parser-results-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prelude = "use std::borrow::Cow;trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn trace<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}";
    let mut failures = Vec::new();
    for (name, body) in [
        ("direct", "Cow::Borrowed(*input)"),
        ("constant", "Cow::Borrowed(\"\")"),
        (
            "assigned",
            "let mut c=Cow::Borrowed(\"\");if !input.is_empty(){c=Cow::Borrowed(*input);}c",
        ),
        (
            "owned_branch",
            "let mut c=Cow::Borrowed(\"\");if !input.is_empty(){c=Cow::Borrowed(*input);}c.to_mut().push_str(\"x\");c",
        ),
    ] {
        let source = format!(
            "{prelude}fn f<'a>(input:&mut &'a str)->Cow<'a,str>{{trace(|input:&mut &'a str|{{{body}}}).next(input)}}fn main(){{}}"
        );
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        assert!(
            typed.diagnostics.is_empty(),
            "{name}: {:?}",
            typed.diagnostics
        );
        let mir = flux_mir::build_file(&hir, &typed);
        if !mir.diagnostics.is_empty() {
            std::fs::write(root.join(format!("{name}.mir")), mir.debug_dump(&hir)).unwrap();
            failures.push((name, mir.diagnostics));
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn opaque_public_result_contracts_reject_escapes() {
    let root = std::env::temp_dir().join(format!(
        "flux-opaque-result-boundaries-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let parser = "use std::borrow::Cow;trait Parser<I,O>{fn next(&mut self,i:&mut I)->O;}impl<I,O,F:FnMut(&mut I)->O>Parser<I,O>for F{fn next(&mut self,i:&mut I)->O{self(i)}}fn trace<I,O,P:Parser<I,O>>(p:P)->impl Parser<I,O>{p}";
    let callable = "fn make<'a>(text:&'a str)->impl Fn()->&'a str{move||text}fn invoke<'a,F:Fn()->&'a str>(f:F)->&'a str{f()}";
    for (name, prefix, source, accepted) in [
        (
            "opaque_callable",
            callable,
            "fn good<'a>(s:&'a str)->&'a str{invoke(make(s))}fn main(){}",
            true,
        ),
        (
            "opaque_callable_escape",
            callable,
            "fn bad()->&'static str{let s=String::from(\"local\");invoke(make(&s))}fn main(){}",
            false,
        ),
        (
            "opaque_callable_static",
            callable,
            "fn require<F:Fn()->&'static str>(f:F)->&'static str{f()}fn bad<'a>(s:&'a str)->&'static str{require(make(s))}fn main(){}",
            false,
        ),
        (
            "parser_output_static",
            parser,
            "fn bad<'a>(input:&mut &'a str)->Cow<'static,str>{trace(|input:&mut &'a str|Cow::Borrowed(*input)).next(input)}fn main(){}",
            false,
        ),
        (
            "parser_local_capture",
            parser,
            "fn bad<'a>(input:&mut &'a str)->Cow<'a,str>{let local=String::from(\"local\");trace(|_:&mut &'a str|Cow::Borrowed(local.as_str())).next(input)}fn main(){}",
            false,
        ),
    ] {
        let source = format!("{prefix}{source}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
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
            accepted,
            "{name}: {:?}",
            mir.diagnostics
        );
    }
}
