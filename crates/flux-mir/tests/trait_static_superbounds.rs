//! Trait Self outlives bounds are inherited without weakening impl or caller checks.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn static_superbounds_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-static-superbounds-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        (
            "direct",
            "trait P:'static{}fn need<T:'static>(){}fn f<T:P>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "where_spelling",
            "trait P where Self:'static{}fn need<T:'static>(){}fn f<T:P>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "transitive",
            "trait P:'static{}trait C:P{}trait G:C{}fn need<T:'static>(){}fn f<T:G>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "parent_after_child",
            "trait G:C{}trait C:P{}trait P:'static{}fn need<T:'static>(){}fn f<T:G>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "diamond",
            "trait A:P{}trait B:P{}trait C:A+B{}trait P:'static{}fn need<T:'static>(){}fn f<T:C>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "where_self_parent",
            "trait P:'static{}trait C where Self:P{}fn need<T:'static>(){}fn f<T:C>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "default_method",
            "trait C:P{fn run(&self){fn need<T:?Sized+'static>(){}need::<Self>();}}trait P:'static{}impl P for u32{}impl C for u32{}fn main(){7u32.run();}",
            true,
        ),
        (
            "generic_parent",
            "trait P<T>:'static{}trait C<T>:P<T>{}fn need<T:'static>(){}fn f<T:C<u8>>(){need::<T>();}fn main(){}",
            true,
        ),
        (
            "bounded_impl",
            "trait P:'static{}struct W<T>(T);impl<T:'static>P for W<T>{}fn main(){}",
            true,
        ),
        (
            "blanket_child",
            "trait P:'static{}trait C:P{}impl<T:P>C for T{}impl P for u32{}fn main(){}",
            true,
        ),
        (
            "static_borrow",
            "trait P:'static{}impl P for &'static u32{}fn main(){}",
            true,
        ),
        (
            "unsafe_marker",
            "unsafe trait Pod:Copy+'static{}unsafe impl Pod for u32{}fn f<T:Pod>(x:T)->T{x}fn main(){assert_eq!(f(3),3);}",
            true,
        ),
        (
            "trait_arg_is_not_self",
            "trait P<T:'static>{}trait C<T:'static>:P<T>{}impl<'a>P<u32> for &'a u32{}impl<'a>C<u32> for &'a u32{}fn main(){}",
            true,
        ),
        (
            "borrowed_impl",
            "trait P:'static{}struct W<'a>(&'a u32);impl<'a>P for W<'a>{}fn main(){}",
            false,
        ),
        (
            "unbounded_generic_impl",
            "trait P:'static{}struct W<T>(T);impl<T>P for W<T>{}fn main(){}",
            false,
        ),
        (
            "child_generic_impl",
            "trait P:'static{}trait C:P{}struct W<T>(T);impl<T:'static>P for W<T>{}impl<T>C for W<T>{}fn main(){}",
            false,
        ),
        (
            "caller_escape",
            "trait P:'static{}trait C:P{}impl<T:'static>P for T{}impl<T:P>C for T{}fn take<T:C>(_:T){}fn main(){let x=7;take(&x);}",
            false,
        ),
        (
            "owned_payload_escape",
            "trait P:'static{}struct W<'a>(&'a u32);impl Drop for W<'_>{fn drop(&mut self){let _=*self.0;}}impl<T:'static>P for T{}fn take<T:P>(_:T){}fn main(){let x=7;take(W(&x));}",
            false,
        ),
        (
            "trait_arg_does_not_imply_self_static",
            "trait P<T:'static>{}trait C<T:'static>:P<T>{}fn need<T:'static>(){}fn f<T:C<u32>>(){need::<T>();}fn main(){}",
            false,
        ),
        (
            "undeclared_lifetime",
            "trait P:'missing{}fn main(){}",
            false,
        ),
        ("anonymous_lifetime", "trait P:'_{}fn main(){}", false),
        (
            "trait_cycle",
            "trait P:C+'static{}trait C:P{}fn main(){}",
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
        let mut diagnostics = hir.diagnostics.clone();
        if diagnostics.is_empty() {
            let typed = flux_mir::check_file(&hir);
            diagnostics.extend(typed.diagnostics.iter().cloned());
            if diagnostics.is_empty() {
                diagnostics.extend(flux_mir::build_file(&hir, &typed).diagnostics);
            }
        }
        assert_eq!(diagnostics.is_empty(), accepted, "{name}: {diagnostics:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn named_self_superbounds_are_represented() {
    let source = "trait P<'a>:'a{fn get(&'a self)->&'a u32;}fn main(){}";
    let root = std::env::temp_dir().join(format!("flux-named-superbound-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("probe.rs");
    std::fs::write(&path, source).unwrap();
    let oracle = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&path)
        .arg("-o")
        .arg(root.join("oracle.rmeta"))
        .output()
        .unwrap();
    assert!(
        oracle.status.success(),
        "{}",
        String::from_utf8_lossy(&oracle.stderr)
    );
    let (_, _, hir) = flux_hir::lower_text(&path, source);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    std::fs::remove_dir_all(root).unwrap();
}
