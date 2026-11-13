#[test]
fn conversion_context_keeps_trait_and_pin_requirements() {
    for (name, source) in [
        (
            "non_send",
            "use std::{pin::Pin,rc::Rc};trait V{}struct S(Rc<()>);impl V for S{}fn main(){let _:Pin<Box<dyn V+Send>>=Pin::from(Box::new(S(Rc::new(()))));}",
        ),
        (
            "unpin",
            "use std::{pin::Pin,marker::PhantomPinned};struct S(PhantomPinned);fn main(){let p=Pin::from(Box::new(S(PhantomPinned)));let _=Pin::into_inner(p);}",
        ),
        (
            "owner_kind",
            "use std::pin::Pin;trait V{}impl V for u32{}fn main(){let _:Pin<&dyn V>=Pin::from(Box::new(1u32));}",
        ),
    ] {
        let root =
            std::env::temp_dir().join(format!("flux-pin-context-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("case.rs");
        std::fs::write(&path, source).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert!(!oracle.status.success(), "rustc accepted {name}");
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let mut errors = typed.diagnostics.clone();
        if errors.is_empty() {
            errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
        }
        assert!(!errors.is_empty(), "flux accepted {name}");
        std::fs::remove_dir_all(root).unwrap();
    }
}
