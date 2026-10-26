//! Residual checks wait for an inferred closure return, preserving conversions.
#[test]
fn inferred_closure_residuals_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-closure-residuals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "option",
            "fn main(){let f=|x:Option<u8>|{let v=x?;Some(v)};assert_eq!(f(Some(7)),Some(7));}",
            true,
        ),
        (
            "result",
            "fn main(){let f=|x:Result<u8,u32>|{let v=x?;Ok::<u8,u32>(v)};assert_eq!(f(Ok(7)),Ok(7));}",
            true,
        ),
        (
            "conversion",
            "struct A;struct B;impl From<A> for B{fn from(_:A)->B{B}}fn main(){let f=|x:Result<u8,A>|{let v=x?;Ok::<u8,B>(v)};let _=f(Err(A));}",
            true,
        ),
        (
            "wrong_return",
            "fn main(){let f=|x:Option<u8>|{let v=x?;v};}",
            false,
        ),
        (
            "wrong_residual",
            "fn main(){let f=|x:Option<u8>|{let v=x?;Ok::<u8,u32>(v)};}",
            false,
        ),
        (
            "missing_conversion",
            "struct A;struct B;fn main(){let f=|x:Result<u8,A>|{let v=x?;Ok::<u8,B>(v)};}",
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
