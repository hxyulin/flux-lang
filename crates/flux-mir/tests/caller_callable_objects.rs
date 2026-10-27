//! Caller Fn bounds must prove every fresh dynamic input lifetime.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;
#[test]
fn caller_callable_objects_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-caller-callable-objects-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "generic",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<F:Fn(&u32)->bool>(f:F){take(&f);}fn main(){}",
            true,
        ),
        (
            "apit",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt(f:impl Fn(&u32)->bool){take(&f);}fn main(){}",
            true,
        ),
        (
            "borrowed_generic",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<F:Fn(&u32)->bool>(f:&F){take(&f);}fn main(){}",
            true,
        ),
        (
            "fn_mut",
            "fn take(_: &mut dyn FnMut(&mut u32)){}fn adapt<F:FnMut(&mut u32)>(mut f:F){take(&mut f);}fn main(){}",
            true,
        ),
        (
            "captured_source",
            "fn adapt<F:Fn(&u32)->bool>(f:&F){let p:&dyn Fn(&u32)->bool=f;let x=3;assert!(p(&x));}fn main(){let x=3;adapt(&|y|*y==x);}",
            true,
        ),
        (
            "two_inputs",
            "fn adapt<F:Fn(&u32,&u64)->bool>(f:F){let _: &dyn Fn(&u32,&u64)->bool=&f;}fn main(){}",
            true,
        ),
        (
            "boxed",
            "fn adapt<F:Fn(&u32)->bool+'static>(f:F)->Box<dyn Fn(&u32)->bool>{Box::new(f)}fn main(){}",
            true,
        ),
        (
            "nested_valid",
            "fn adapt<F:Fn(&u32)->bool>(f:F){let g=||{let p:&dyn Fn(&u32)->bool=&f;assert!(p(&7));};g();}fn main(){}",
            true,
        ),
        (
            "nested_fixed",
            "fn adapt<'a,F:Fn(&'a u32)->bool>(f:F){let g=||{let _: &dyn Fn(&u32)->bool=&f;};g();}fn main(){}",
            false,
        ),
        (
            "associated_source",
            "trait T{type Callback;}fn adapt<R:T>(f:R::Callback)where R::Callback:Fn(&u32)->bool{let _: &dyn Fn(&u32)->bool=&f;}fn main(){}",
            true,
        ),
        (
            "associated_fixed",
            "trait T{type Callback;}fn adapt<'a,R:T>(f:R::Callback)where R::Callback:Fn(&'a u32)->bool{let _: &dyn Fn(&u32)->bool=&f;}fn main(){}",
            false,
        ),
        (
            "static_only",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<F:Fn(&'static u32)->bool>(f:F){take(&f);}fn main(){}",
            false,
        ),
        (
            "fixed_lifetime",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<'a,F:Fn(&'a u32)->bool>(f:F){take(&f);}fn main(){}",
            false,
        ),
        (
            "borrowed_fixed",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<'a,F:Fn(&'a u32)->bool>(f:&F){take(&f);}fn main(){}",
            false,
        ),
        (
            "wrong_output",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<F:Fn(&u32)->u32>(f:F){take(&f);}fn main(){}",
            false,
        ),
        (
            "fnmut_not_fn",
            "fn take(_: &dyn Fn(&u32)->bool){}fn adapt<F:FnMut(&u32)->bool>(f:F){take(&f);}fn main(){}",
            false,
        ),
        (
            "escaped_source",
            "fn adapt<'a,F:Fn(&u32)->bool+'a>(f:&'a F)->&'static dyn Fn(&u32)->bool{f}fn main(){}",
            false,
        ),
        (
            "unsafe_callback",
            "fn adapt<F:Fn(&u32)->bool>(_:F){}unsafe fn bad(_: &u32)->bool{true}fn main(){adapt(bad);}",
            false,
        ),
        (
            "c_callback",
            "fn adapt<F:Fn(&u32)->bool>(_:F){}extern \"C\" fn bad(_: &u32)->bool{true}fn main(){adapt(bad);}",
            false,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut errors = db.syntax_errors();
        errors.extend(hir.diagnostics.clone());
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}
