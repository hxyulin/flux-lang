//! Bound callable input references cannot escape into captured storage.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn callable_object_inputs_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-callable-objects-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "borrowed",
            "fn apply(f:&dyn Fn(&u64)->u64)->u64 { let x=17; f(&x) } fn main(){ let offset=3; assert_eq!(apply(&|x| *x+offset),20); }",
            true,
        ),
        (
            "mutable",
            "fn apply(f:&dyn Fn(&mut u64)->u64)->u64 { let mut x=17; f(&mut x) } fn main(){ assert_eq!(apply(&|x| { *x+=3; *x }),20); }",
            true,
        ),
        (
            "independent",
            "fn apply(f:&dyn Fn(&u64,&u64)->u64)->u64 { let x=3; let y=7; f(&x,&y) } fn main(){ assert_eq!(apply(&|x,y| *x+*y),10); }",
            true,
        ),
        (
            "capture_input",
            "fn main(){ let mut saved=None; { let mut f=|x:&u64| { saved=Some(x); }; let _: &mut dyn FnMut(&u64)=&mut f; } println!(\"{:?}\",saved); }",
            false,
        ),
        (
            "require_static",
            "fn need(_: &'static u64) {} fn main(){ let f=|x:&u64| need(x); let _: &dyn Fn(&u64)=&f; }",
            false,
        ),
        (
            "mut_ref_not_fn",
            "fn need<F:Fn(u64)->u64>(f:F){let _=f(3);} fn main(){let mut f=|x|x+1; need(&mut f);}",
            false,
        ),
        (
            "static_function",
            "fn keep(_: &'static u64) {} fn main(){let _: &dyn Fn(&u64)=&keep;}",
            false,
        ),
        (
            "unsafe_pointer",
            "unsafe fn f(x:i32)->i32{x} fn main(){let p:unsafe fn(i32)->i32=f;(&p)(3);}",
            false,
        ),
        (
            "c_pointer_object",
            "extern \"C\" fn f(x:i32)->i32{x} fn main(){let p:extern \"C\" fn(i32)->i32=f;let _: &dyn Fn(i32)->i32=&p;}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
