//! Standard conversion contracts retain owned values and borrowed lifetimes.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn assembler_contracts_match_rustc() {
    let root =
        std::env::temp_dir().join(format!("flux-assembler-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let error = "#[derive(Debug)]struct E<'a>(&'a str);impl std::fmt::Display for E<'_>{fn fmt(&self,f:&mut std::fmt::Formatter<'_>)->std::fmt::Result{f.write_str(self.0)}}impl std::error::Error for E<'_>{}";
    for (name, source, accepted) in [
        ("borrowed_error", format!("{error}fn f<'a>(x:&'a str)->Box<dyn std::error::Error+'a>{{E(x).into()}}fn main(){{}}"), true),
        ("escaping_error", format!("{error}fn f<'a>(x:&'a str)->Box<dyn std::error::Error>{{E(x).into()}}fn main(){{}}"), false),
        ("not_an_error", "struct E;fn main(){let _:Box<dyn std::error::Error>=E.into();}".into(), false),
        ("clone_bound", "struct N;fn main(){let n=N;let _=Ok::<&N,() >(&n).cloned();}".into(), false),
        ("phantom_unsized", "struct N;fn main(){let _:std::marker::PhantomData<[N]>=Default::default();}".into(), true),
        ("noncopy_slice", "struct N;fn main(){let mut n=[N];let _:[N;1]=(&mut n[..]).try_into().unwrap();}".into(), false),
        ("cow_escape", "fn f()->std::borrow::Cow<'static,str>{let s=String::from(\"x\");std::borrow::Cow::Borrowed(&s)}fn main(){}".into(), false),
    ] {
        if !case_filter::selected(name) { continue; }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = std::process::Command::new("rustc").args(["--edition=2024", "--emit=metadata", "--out-dir"]).arg(&root).arg(&path).output().unwrap();
        assert_eq!(reference.status.success(), accepted, "rustc {name}: {}", String::from_utf8_lossy(&reference.stderr));
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        let mut errors:Vec<_> = hir.diagnostics.iter().map(|d|d.message.clone()).collect();
        if errors.is_empty() {
            let typed = flux_mir::check_file(&hir);
            errors.extend(typed.diagnostics.iter().map(|d|d.message.clone()));
            if errors.is_empty() { errors.extend(flux_mir::build_file(&hir,&typed).diagnostics.into_iter().map(|d|d.message)); }
        }
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
