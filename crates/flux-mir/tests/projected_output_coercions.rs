//! Associated outputs are selected before applying reference coercions.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-projected-output-{name}-{}",
        std::process::id()
    ));
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
fn borrowed_vec_output() {
    compare(
        "borrowed_vec_output",
        r#"fn f(v: &Vec<Vec<u8>>, i: usize) -> &[u8] { unsafe { v.get_unchecked(i) } }"#,
        true,
    );
}

#[test]
fn constructor_output() {
    compare(
        "constructor_output",
        r#"struct P<'a>(&'a [u8]); fn f(v: &Vec<Vec<u8>>, i: usize) -> P<'_> { P(unsafe { v.get_unchecked(i) }) }"#,
        true,
    );
}

#[test]
fn mutable_output() {
    compare(
        "mutable_output",
        r#"fn f(v: &mut Vec<Vec<u8>>, i: usize) -> &mut [u8] { unsafe { v.get_unchecked_mut(i) } }"#,
        true,
    );
}

#[test]
fn wrong_element() {
    compare(
        "wrong_element",
        r#"fn f(v: &Vec<Vec<u8>>, i: usize) -> &[u16] { unsafe { v.get_unchecked(i) } }"#,
        false,
    );
}

#[test]
fn shared_to_mutable() {
    compare(
        "shared_to_mutable",
        r#"fn f(v: &Vec<Vec<u8>>, i: usize) -> &mut [u8] { unsafe { v.get_unchecked(i) } }"#,
        false,
    );
}

#[test]
fn output_escape() {
    compare(
        "output_escape",
        r#"fn f(v: &Vec<Vec<u8>>, i: usize) -> &'static [u8] { unsafe { v.get_unchecked(i) } }"#,
        false,
    );
}

#[test]
fn ordinary_generic_hint() {
    compare(
        "ordinary_generic_hint",
        r#"fn f(v: Option<&[u8]>) -> &[u8] { v.map_or(&[], |x| x) }"#,
        true,
    );
}
