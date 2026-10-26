//! Associated callable inputs normalize before forwarded FnOnce outputs.
#[test]
fn forwarded_callable_projections_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-forwarded-callable-projections-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "copied",
            "fn visit(mut f:impl FnMut(u32)){[1u32,2].iter().copied().for_each(&mut f);}fn main(){let mut n=0;visit(|v|n+=v);assert_eq!(n,3);}",
            true,
        ),
        (
            "array",
            "fn visit<F:FnMut(u32)>(mut f:F){[1u32,2].into_iter().for_each(&mut f);}fn main(){let mut n=0;visit(|v|n+=v);assert_eq!(n,3);}",
            true,
        ),
        (
            "mut_refs",
            "fn visit(mut f:impl FnMut(&mut u32)){let mut values=[1u32,2];values.iter_mut().for_each(&mut f);values.iter_mut().for_each(f);}fn main(){visit(|v|*v+=7);}",
            true,
        ),
        (
            "direct",
            "fn call<F:FnOnce(u32)>(f:F){f(3);}fn visit(mut f:impl FnMut(u32)){call(&mut f);}fn main(){let mut n=0;visit(|v|n+=v);assert_eq!(n,3);}",
            true,
        ),
        (
            "wrong_output",
            "fn visit(mut f:impl FnMut(u32)->u32){[1u32].into_iter().for_each(&mut f);}fn main(){}",
            false,
        ),
        (
            "shared_mut_only",
            "fn visit<F:FnMut(u32)>(f:&F){[1u32].into_iter().for_each(f);}fn main(){}",
            false,
        ),
        (
            "c_pointer",
            "extern \"C\" fn c(_:u32){}fn main(){let mut f:extern \"C\" fn(u32)=c;[1u32].into_iter().for_each(&mut f);}",
            false,
        ),
        (
            "unsafe_pointer",
            "unsafe fn unsafe_f(_:u32){}fn main(){let mut f:unsafe fn(u32)=unsafe_f;[1u32].into_iter().for_each(&mut f);}",
            false,
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
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
        assert_eq!(errors.is_empty(), accepted, "{name}: {errors:?}");
    }
}
