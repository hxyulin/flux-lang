//! Well-formed family inputs carry declaration prerequisites, not unrelated facts.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn family_well_formed_inputs_match_rustc() {
    let root = std::env::temp_dir().join(format!(
        "flux-family-well-formed-inputs-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "trait F{type Out<'a,T>where T:'static,Self:'a;}fn need<T:'static>(_:core::marker::PhantomData<T>){}fn need_value<T:'static>(_:T){}";
    for (name, declaration, accepted) in [
        (
            "family_input_type",
            "fn input<'a,T,O:F>(x:&'a(),_:O::Out<'a,T>){need::<T>(core::marker::PhantomData::<T>)}",
            true,
        ),
        (
            "nested_family_input_type",
            "fn input<'a,T,O:F>(x:&'a(),_:Option<O::Out<'a,T>>){need::<T>(core::marker::PhantomData::<T>)}",
            true,
        ),
        (
            "family_argument_regions",
            "fn input<'a,'b,O:F>(x:&'a(),_:O::Out<'a,&'b u8>,value:&'b u8){need_value(value)}",
            true,
        ),
        (
            "unrelated_family_argument",
            "fn input<'a,T,U,O:F>(x:&'a(),_:O::Out<'a,U>){need::<T>(core::marker::PhantomData::<T>)}",
            false,
        ),
        (
            "missing_family_input",
            "fn input<'a,T>(x:&'a()){need::<T>(core::marker::PhantomData::<T>)}",
            false,
        ),
        (
            "selected_output_applicability",
            "struct S;impl F for S{type Out<'a,T>=T where T:'static;}fn bad<'b>(value:&'b u8)-><S as F>::Out<'static,&'b u8>{value}",
            false,
        ),
        (
            "local_family_annotation",
            "struct S;impl F for S{type Out<'a,T>=T where T:'static;}fn bad(){let local=1u8;let value:Option<<S as F>::Out<'static,&u8>>=Some(&local);let _=value;}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{prefix}{declaration}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
        let reference = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "--out-dir"])
            .arg(&root)
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
        let mut errors: Vec<_> = hir.diagnostics.iter().map(|d| d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d| d.message.clone()));
            if errors.is_empty() {
                errors.extend(
                    flux_mir::build_file(&hir, &typed)
                        .diagnostics
                        .into_iter()
                        .map(|d| d.message),
                );
            }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
