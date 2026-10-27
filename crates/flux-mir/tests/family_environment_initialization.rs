//! Selected families retain the original function and impl lifetime facts.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn family_selection_uses_original_header_facts() {
    let root = std::env::temp_dir().join(format!("flux-family-environment-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let prefix = "trait Family{type Out<'a>;}struct Wrap<F>(F);impl<F:FnMut(&u32)->u32>Family for Wrap<F>{type Out<'a>=&'a u32;}";
    for (name, declaration, accepted) in [
        (
            "signature_family",
            "fn f<'a,F:FnMut(&u32)->u32>(x:&'a u32)-><Wrap<F>as Family>::Out<'a>{x}",
            true,
        ),
        (
            "signature_original_outlives",
            "fn f<'a,'b:'a,F:FnMut(&u32)->u32>(x:&'b u32)-><Wrap<F>as Family>::Out<'a>{x}",
            true,
        ),
        (
            "signature_missing_outlives",
            "fn f<'a,'b,F:FnMut(&u32)->u32>(x:&'b u32)-><Wrap<F>as Family>::Out<'a>{x}",
            false,
        ),
        (
            "signature_static_escape",
            "fn f<'a,F:FnMut(&u32)->u32>(x:&'a u32)-><Wrap<F>as Family>::Out<'static>{x}",
            false,
        ),
        (
            "bound_family",
            "fn f<'a,F:FnMut(&u32)->u32,I:Iterator<Item=<Wrap<F>as Family>::Out<'a>>>(_:I){}",
            true,
        ),
        (
            "header_constant",
            "impl<'a,F:FnMut(&u32)->u32>Wrap<F>{const VALUE:Option<<Wrap<F>as Family>::Out<'a>>=None;}",
            true,
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
