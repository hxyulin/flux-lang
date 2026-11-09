//! Raw pointer casts preserve written nested reference lifetime constraints.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-raw-cast-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("case.rs");
    let source = format!("{source}\nfn main() {{}}\n");
    std::fs::write(&path, &source).unwrap();
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
    let (_, _, hir) = flux_hir::lower_text(&path, &source);
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
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
}

#[test]
fn raw_cast_static() {
    compare(
        "raw_cast_static",
        r#"unsafe fn f(p:*const &u32)-> &'static u32 {unsafe {*(p as *const &'static u32)}}"#,
        true,
    );
}

#[test]
fn raw_cast_named() {
    compare(
        "raw_cast_named",
        r#"unsafe fn f<'a>(p:*const &u32)-> &'a u32 {unsafe {*(p as *const &'a u32)}}"#,
        true,
    );
}

#[test]
fn reference_cast_cannot_extend() {
    compare(
        "reference_cast_cannot_extend",
        r#"fn f<'a>(x:&'a u32)->*const &'static u32 {&x as *const &'static u32}"#,
        false,
    );
}

#[test]
fn cast_cannot_return_local() {
    compare(
        "cast_cannot_return_local",
        r#"fn f<'a>(x:&'a u32)-> &'static u32 {unsafe{*(&x as *const &'static u32)}}"#,
        false,
    );
}

#[test]
fn reference_cast_shortening() {
    compare(
        "reference_cast_shortening",
        r#"fn f<'a,'b:'a>(x:&'b u32)->*const &'a u32 {&x as *const &'a u32}"#,
        true,
    );
}

#[test]
fn undeclared_cast_lifetime() {
    compare(
        "undeclared_cast_lifetime",
        r#"unsafe fn f(p:*const &u32) {let _=p as *const &'missing u32;}"#,
        false,
    );
}

#[test]
fn static_raw_cast_in_closure() {
    compare(
        "static_raw_cast_in_closure",
        r#"unsafe fn f(p:*const &u32)-> &'static u32 {(||unsafe{*(p as *const &'static u32)})()}"#,
        true,
    );
}

#[test]
fn object_pointer_cast_cannot_extend_bound() {
    compare(
        "object_bound",
        "trait A{}fn f<'a>(x:*const(dyn A+'a))->*const dyn A{x as *const dyn A}",
        false,
    );
}

#[test]
fn object_reference_cast_cannot_extend_bound() {
    compare(
        "object_reference_bound",
        "trait A{}fn f<'a>(x:&'a(dyn A+'a))->*const dyn A{x as *const dyn A}",
        false,
    );
}

#[test]
fn nested_object_pointer_cast_is_reinterpretation() {
    compare(
        "nested_object",
        "trait A{}fn f<'a>(x:*const *const(dyn A+'a))->*const *const dyn A{x as *const *const dyn A}",
        true,
    );
}
