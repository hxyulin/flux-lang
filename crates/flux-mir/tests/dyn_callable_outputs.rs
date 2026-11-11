//! Callable object outputs retain input binders and captured regions.
#[test]
fn callable_object_outputs_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-dyn-call-output-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepted) in [
        (
            "dyn_borrowed_output",
            "type Callback<'a>=&'a dyn Fn(&str)->&str;fn apply<'a>(f:Callback<'_>,x:&'a str)->&'a str{f(x)}",
            true,
        ),
        (
            "dyn_cow_output",
            "use std::borrow::Cow;type Callback<'a>=Option<&'a dyn Fn(&str)->Cow<'_,[u8]>>;fn apply<'a>(f:Callback<'_>,x:&'a str)->Cow<'a,[u8]>{if let Some(f)=f{return f(x)}x.as_bytes().into()}",
            true,
        ),
        (
            "dyn_static_escape",
            "fn bad(f:&dyn Fn(&str)->&str,x:&str)->&'static str{f(x)}",
            false,
        ),
        (
            "dyn_local_escape",
            "fn bad<'a>(f:&dyn Fn(&str)->&str)->&'a str{let s=String::from(\"local\");f(&s)}",
            false,
        ),
        (
            "dyn_independent_capture",
            "fn apply<'a,'b>(f:&dyn Fn(&'a str)->&'b str,x:&'a str)->&'b str{f(x)}",
            true,
        ),
        (
            "dyn_capture_escape",
            "fn bad<'a,'b>(f:&dyn Fn(&'a str)->&'b str,x:&'a str)->&'static str{f(x)}",
            false,
        ),
        (
            "named_pointer_cast",
            "fn select<'a>(x:&'a u8)->Option<&'a u8>{Some(x)}fn apply<'a>(x:&'a[u8])->Option<&'a u8>{x.iter().filter_map(select as fn(&'a u8)->Option<&'a u8>).next()}",
            true,
        ),
        (
            "pointer_cast_local_escape",
            "fn id(x:&u8)->&u8{x}fn bad<'a>()->&'a u8{let x=3;(id as fn(&'a u8)->&'a u8)(&x)}",
            false,
        ),
        (
            "pointer_cast_static_escape",
            "fn id(x:&u8)->&u8{x}fn bad<'a>(x:&'a u8)->&'static u8{(id as fn(&'a u8)->&'a u8)(x)}",
            false,
        ),
        (
            "pointer_cast_static_input",
            "fn id(x:&u8)->&u8{x}fn bad(x:&u8)->&u8{(id as fn(&'static u8)->&'static u8)(x)}",
            false,
        ),
        (
            "pointer_cast_closure_owner",
            "fn select<'a>(x:&'a u8)->Option<&'a u8>{Some(x)}fn apply<'a>(x:&'a[u8])->Option<&'a u8>{Some(x).and_then(|x|x.iter().filter_map(select as fn(&'a u8)->Option<&'a u8>).next())}",
            true,
        ),
        (
            "function_object_output",
            "fn id(x:&str)->&str{x}fn make(){let _f:&dyn Fn(&str)->&str=&id;}",
            true,
        ),
        (
            "function_object_wrong_output",
            "fn last<'a,'b>(_:&'a str,b:&'b str)->&'b str{b}fn make(){let _f:&dyn for<'a,'b>Fn(&'a str,&'b str)->&'a str=&last;}",
            false,
        ),
        (
            "mutable_object_output",
            "fn apply<'a>(f:&mut dyn FnMut(&str)->&str,x:&'a str)->&'a str{f(x)}",
            true,
        ),
        (
            "array_context_keeps_invariance",
            "trait Value{}impl Value for u8{}fn bad(){let value=1u8;let a=vec![&value as &dyn Value];let b=vec![&value];let _=[a,b];}",
            false,
        ),
        (
            "generic_object_output",
            "fn forward<'a,F:?Sized+Fn(&str)->&str>(f:&F,x:&'a str)->&'a str{f(x)}fn apply<'a>(f:&dyn Fn(&str)->&str,x:&'a str)->&'a str{forward(f,x)}",
            true,
        ),
        (
            "generic_object_static_escape",
            "fn forward<'a,F:?Sized+Fn(&str)->&str>(f:&F,x:&'a str)->&'a str{f(x)}fn bad(f:&dyn Fn(&str)->&str,x:&str)->&'static str{forward(f,x)}",
            false,
        ),
        (
            "generic_object_not_general",
            "fn forward<'a,F:?Sized+Fn(&str)->&str>(f:&F,x:&'a str)->&'a str{f(x)}fn bad<'a>(f:&dyn Fn(&'a str)->&'a str,x:&'a str)->&'a str{forward(f,x)}",
            false,
        ),
        (
            "generic_object_wrong_output",
            "fn forward<F:?Sized+Fn(&str)->&'static str>(f:&F,x:&str)->&'static str{f(x)}fn bad(f:&dyn Fn(&str)->&str,x:&str)->&'static str{forward(f,x)}",
            false,
        ),
        (
            "generic_pointer_output",
            "fn forward<'a,F:Fn(&str)->&str>(f:F,x:&'a str)->&'a str{f(x)}fn apply<'a>(f:fn(&str)->&str,x:&'a str)->&'a str{forward(f,x)}",
            true,
        ),
    ] {
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args([
                "--edition=2024",
                "--crate-type=lib",
                "--emit=metadata",
                "--out-dir",
            ])
            .arg(&directory)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        assert!(
            db.syntax_errors().is_empty(),
            "{name}: {:?}",
            db.syntax_errors()
        );
        let typed = flux_mir::check_file(&hir);
        let mut diagnostics = hir.diagnostics.clone();
        diagnostics.extend(typed.diagnostics.iter().cloned());
        if diagnostics.is_empty() {
            diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(
            diagnostics.is_empty(),
            accepted,
            "flux {name}: {diagnostics:?}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
}
