//! Trusted NonNull coercions preserve element types and region constraints.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-nonnull-{name}-{}", std::process::id()));
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
fn array_slice() {
    compare(
        "array_slice",
        r#"fn f(x: std::ptr::NonNull<[u32;3]>) -> std::ptr::NonNull<[u32]> { x }"#,
        true,
    );
}

#[test]
fn empty_array() {
    compare(
        "empty_array",
        r#"fn f() { let _: std::ptr::NonNull<[u32]> = std::ptr::NonNull::<[u32;0]>::dangling(); }"#,
        true,
    );
}

#[test]
fn wrong_element() {
    compare(
        "wrong_element",
        r#"fn f(x: std::ptr::NonNull<[u16;3]>) -> std::ptr::NonNull<[u32]> { x }"#,
        false,
    );
}

#[test]
fn lookalike() {
    compare(
        "lookalike",
        r#"struct NonNull<T: ?Sized>(*const T); fn f(x: NonNull<[u32;3]>) -> NonNull<[u32]> { x }"#,
        false,
    );
}

#[test]
fn covariant_shortening() {
    compare(
        "covariant_shortening",
        r#"fn f<'a, 'b: 'a>(x: std::ptr::NonNull<[&'b u32;3]>) -> std::ptr::NonNull<[&'a u32]> { x }"#,
        true,
    );
}

#[test]
fn covariant_escape() {
    compare(
        "covariant_escape",
        r#"fn f<'a, 'b>(x: std::ptr::NonNull<[&'b u32;3]>) -> std::ptr::NonNull<[&'a u32]> { x }"#,
        false,
    );
}
