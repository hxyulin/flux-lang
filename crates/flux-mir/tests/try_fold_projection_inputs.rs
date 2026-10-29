//! Associated equality inputs reuse their exact original owning predicate.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
#[test]
fn try_fold_projection_inputs_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-try-fold-inputs-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "shared_option_fold",
            "fn f<'a>(value:&'a u32)->Option<&'a u32>{[()].iter().try_fold(value,|target,_|Some(target))}",
            true,
        ),
        (
            "mutable_option_fold",
            "fn f<'a>(value:&'a mut u32)->Option<&'a mut u32>{[()].iter().try_fold(value,|target,_|Some(target))}",
            true,
        ),
        (
            "shared_result_fold",
            "fn f<'a>(value:&'a u32)->Result<&'a u32,()>{[()].iter().try_fold(value,|target,_|Ok(target))}",
            true,
        ),
        (
            "control_flow_fold",
            "fn f<'a>(value:&'a u32)->core::ops::ControlFlow<(),&'a u32>{[()].iter().try_fold(value,|target,_|core::ops::ControlFlow::Continue(target))}",
            true,
        ),
        (
            "shared_fold_static_escape",
            "fn f<'a>(value:&'a u32)->Option<&'static u32>{[()].iter().try_fold(value,|target,_|Some(target))}",
            false,
        ),
        (
            "mutable_fold_static_escape",
            "fn f<'a>(value:&'a mut u32)->Option<&'static mut u32>{[()].iter().try_fold(value,|target,_|Some(target))}",
            false,
        ),
        (
            "temporary_fold_escape",
            "fn f()->Option<&'static u32>{let value=7;[()].iter().try_fold(&value,|target,_|Some(target))}",
            false,
        ),
        (
            "direct_owner",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}fn extract<B,R:Own<Output=B>>(r:R)->B{r.output()}fn f<'a>(v:&'a u32)->&'a u32{extract(Holder(v))}",
            true,
        ),
        (
            "inherited_owner",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}trait Child:Own{}impl<'a>Child for Holder<'a>{}fn extract<B,R:Child<Output=B>>(r:R)->B{r.output()}fn f<'a>(v:&'a u32)->&'a u32{extract(Holder(v))}",
            true,
        ),
        (
            "direct_owner_wrong_output",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}fn extract<B,R:Own<Output=B>>(r:R)->B{r.output()}fn f<'a>(v:&'a u32)->&'static u32{extract(Holder(v))}",
            false,
        ),
        (
            "inherited_owner_wrong_output",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}trait Child:Own{}impl<'a>Child for Holder<'a>{}fn extract<B,R:Child<Output=B>>(r:R)->B{r.output()}fn f<'a>(v:&'a u32)->&'static u32{extract(Holder(v))}",
            false,
        ),
        (
            "conditional_owner_missing",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a:'static>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}fn extract<B,R:Own<Output=B>>(r:R)->B{r.output()}fn f<'a>(v:&'a u32)->&'a u32{extract(Holder(v))}",
            false,
        ),
        (
            "conditional_owner_proven",
            "trait Own{type Output;fn output(self)->Self::Output;}struct Holder<'a>(&'a u32);impl<'a:'static>Own for Holder<'a>{type Output=&'a u32;fn output(self)->Self::Output{self.0}}fn extract<B,R:Own<Output=B>>(r:R)->B{r.output()}fn f<'a:'static>(v:&'a u32)->&'a u32{extract(Holder(v))}",
            true,
        ),
        (
            "wrong_fold_output_kind",
            "fn f<'a>(value:&'a u32)->Option<&'a u32>{[()].iter().try_fold(value,|target,_|target)}",
            false,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let source = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let rustc = std::process::Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("case.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepted,
            "rustc {name}:{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors = hir.diagnostics.clone();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            if !accepted && name != "wrong_fold_output_kind" {
                assert!(
                    typed.diagnostics.is_empty(),
                    "expected a MIR lifetime rejection for {name}: {:?}",
                    typed.diagnostics
                );
            }
            errors.extend(typed.diagnostics.clone());
            if errors.is_empty() {
                errors.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        if !accepted && name != "wrong_fold_output_kind" {
            assert!(
                errors
                    .iter()
                    .all(|error| !error.message.contains("not supported")
                        && !error.message.contains("not represented")
                        && !error.message.contains("metadata")),
                "expected a lifetime proof rejection for {name}: {errors:?}"
            );
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}:{errors:?}");
    }
}
