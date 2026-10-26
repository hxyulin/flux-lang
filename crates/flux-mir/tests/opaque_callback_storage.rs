//! Explicit opaque outlives bounds describe storage, not callback argument types.
#[test]
fn opaque_callback_storage_matches_rustc() {
    let root = std::env::temp_dir().join(format!("flux-opaque-callback-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "unused_receiver",
            "struct S(u64); impl S { fn cb<'a>(&self,x:&'a u64)->impl Fn()->u64+'a {move||*x} fn f(&mut self,x:&u64)->u64 {let cb=self.cb(x);self.0+=1;cb()} } fn main(){}",
            true,
        ),
        (
            "signature_types",
            "fn make<'a,T,U>(x:&'a u64)->impl Fn(&(T,U))->u64+'a {move|_|*x} fn main(){}",
            true,
        ),
        (
            "captured_type",
            "fn make<'a,T>(x:T)->impl Fn()->usize+'a {move||{let _=&x;1}} fn main(){}",
            false,
        ),
        (
            "expired_input",
            "fn make<'a>(x:&'a u64)->impl Fn()->u64+'a {move||*x} fn main(){let cb;{let x=7;cb=make(&x);}println!(\"{}\",cb());}",
            false,
        ),
        (
            "expired_type_input",
            "fn make<'a,T:Clone+'a>(x:T)->impl Fn()->T+'a {move||x.clone()} fn main(){let cb;{let x=7;cb=make(&x);}println!(\"{}\",cb());}",
            false,
        ),
    ] {
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
