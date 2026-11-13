//! Pointer patterns require thin integer addresses, not allocation provenance.
fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-pointer-pattern-{name}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    std::fs::write(&path, source).unwrap();
    let rustc = std::process::Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata", "--out-dir"])
        .arg(&root)
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(
        rustc.status.success(),
        accepted,
        "rustc {name}: {}",
        String::from_utf8_lossy(&rustc.stderr)
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
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}
#[test]
fn integer_address_patterns_match() {
    compare(
        "integer",
        include_str!("../../../tests/run/raw_pointer_patterns.rs"),
        true,
    );
}
#[test]
fn raw_pointer_patterns_keep_structural_and_exhaustiveness_checks() {
    for (name, source) in [
        (
            "nonexhaustive",
            "const P:*const u8=0usize as *const u8;const Q:*const u8=1usize as *const u8;fn f(p:*const u8){match p{P=>{},Q=>{}}}fn main(){}",
        ),
        (
            "range",
            "const P:*const u8=0usize as *const u8;const Q:*const u8=1usize as *const u8;fn f(p:*const u8){match p{P..=Q=>{},_=>{}}}fn main(){}",
        ),
        (
            "allocation",
            "static N:u8=7;const P:*const u8=&N as *const u8;fn f(p:*const u8)->bool{matches!(p,P)}fn main(){}",
        ),
        (
            "allocation_aggregate",
            "static N:u8=7;const P:(*const u8,) = (&N as *const u8,);fn f(p:(*const u8,))->bool{matches!(p,P)}fn main(){}",
        ),
        (
            "function",
            "fn g(){}const F:fn()=g;fn f(p:fn())->bool{matches!(p,F)}fn main(){}",
        ),
        (
            "function_address",
            "fn g(){}const F:*const()=g as *const();fn f(p:*const())->bool{matches!(p,F)}fn main(){}",
        ),
        (
            "fat_pointer",
            "const P:*const[u8]=std::ptr::slice_from_raw_parts(0usize as *const u8,0);fn f(p:*const[u8])->bool{matches!(p,P)}fn main(){}",
        ),
    ] {
        compare(name, source, false);
    }
}
