//! Operand and divergent-expression contexts constrain inference before MIR.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod cases;

#[test]
fn backend_core_inference_matches_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-backend-core-inference-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "competing_rhs_impls",
            "struct A;struct B;struct C;impl PartialEq<B> for A{fn eq(&self,_:&B)->bool{true}}impl PartialEq<C> for A{fn eq(&self,_:&C)->bool{false}}fn main(){assert!(!(A==C));}",
            true,
        ),
        (
            "self_and_other_rhs",
            "#[derive(PartialEq)]struct A(u32);impl PartialEq<u64> for A{fn eq(&self,_:&u64)->bool{true}}fn main(){assert!(A(1)==2u64);}",
            true,
        ),
        (
            "generic_constructor_hint",
            "#[derive(PartialEq)]struct A<T>(T);fn main(){assert!(A(2u32)==A(2));}",
            true,
        ),
        (
            "constructor_shared_reborrow",
            "#[derive(PartialEq)]struct A<T>(T);fn main(){let x=3u32;let mut y=3;assert!(A(&x)==A(&mut y));assert_eq!(y,3);}",
            true,
        ),
        (
            "constructor_wrong_type",
            "#[derive(PartialEq)]struct A<T>(T);fn main(){let _=A(1u32)==A(2u64);}",
            false,
        ),
        (
            "divergent_closure",
            "fn take<F:Fn()->u32>(_:F){}fn main(){take(||loop{});take(||{if true{return 1;}loop{}});}",
            true,
        ),
        (
            "divergent_array",
            "fn value()->[u32;1]{[loop{}]}fn main(){}",
            true,
        ),
        (
            "divergent_option",
            "fn value()->Option<u32>{Some(loop{})}fn main(){}",
            true,
        ),
        (
            "closure_wrong_return",
            "fn take<F:Fn()->u32>(_:F){}fn main(){take(||{if true{return \"wrong\";}loop{}});}",
            false,
        ),
        (
            "option_wrong_return",
            "fn value()->Option<u32>{if true{return Some(\"wrong\");}Some(loop{})}fn main(){}",
            false,
        ),
    ] {
        if !cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (db, _, hir) = flux_hir::lower_text(&path, source);
        let mut diagnostics = db.syntax_errors();
        diagnostics.extend(hir.diagnostics.clone());
        if diagnostics.is_empty() {
            let typed = flux_mir::check_file(&hir);
            diagnostics.extend(typed.diagnostics.clone());
            if diagnostics.is_empty() {
                diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(diagnostics.is_empty(), accepted, "{name}: {diagnostics:?}");
    }
}
