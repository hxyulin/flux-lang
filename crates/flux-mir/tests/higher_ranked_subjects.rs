//! Higher-ranked predicate subjects instantiate one correlated binder.

fn compare(name: &str, source: &str, accepted: bool) {
    let root =
        std::env::temp_dir().join(format!("flux-ranked-subject-{name}-{}", std::process::id()));
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
fn universal_subject() {
    compare(
        "universal_subject",
        r#"trait D<'a>{}impl<'a,T>D<'a>for &'a T{}fn need<T>()where for<'a> &'a T:D<'a>{}fn main_call(){need::<u32>();}"#,
        true,
    );
}

#[test]
fn static_subject_rejected() {
    compare(
        "static_subject_rejected",
        r#"trait D{}impl D for &'static u32{}fn need<T>()where for<'a> &'a T:D{}fn main_call(){need::<u32>();}"#,
        false,
    );
}

#[test]
fn correlated_subject() {
    compare(
        "correlated_subject",
        r#"trait D<T>{}impl<'a>D<&'a u32>for &'a u32{}fn need()where for<'a> &'a u32:D<&'a u32>{}fn main_call(){need();}"#,
        true,
    );
}

#[test]
fn distinct_subject_rejected() {
    compare(
        "distinct_subject_rejected",
        r#"trait D<T>{}impl<'a>D<&'a u32>for &'a u32{}fn need()where for<'a,'b> &'a u32:D<&'b u32>{}fn main_call(){need();}"#,
        false,
    );
}

#[test]
fn assumption_use() {
    compare(
        "assumption_use",
        r#"trait D<'a>{}fn need<T>()where for<'a> &'a T:D<'a>{}fn outer<T>()where for<'b> &'b T:D<'b>{need::<T>();}"#,
        true,
    );
}

#[test]
fn fixed_assumption_rejected() {
    compare(
        "fixed_assumption_rejected",
        r#"trait D{}fn need<T>()where for<'a> &'a T:D{}fn outer<'b,T>()where &'b T:D{need::<T>();}"#,
        false,
    );
}

#[test]
fn ordinary_assumption_instantiation() {
    compare(
        "ordinary_assumption_instantiation",
        r#"trait D{}fn inner<T:D>(_:T){}fn outer<T>(x:&T)where for<'a> &'a T:D{inner(x);}"#,
        true,
    );
}

#[test]
fn borrowed_payload_escape() {
    compare(
        "borrowed_payload_escape",
        r#"trait D{}impl<T:'static>D for &T{}fn need<T>(_:T)where for<'a> &'a T:D{}fn outer(x:&u32){need(x);}"#,
        false,
    );
}

#[test]
fn subject_does_not_make_payload_static() {
    compare(
        "no_global_static",
        "trait D{} fn need<T:'static>(_:T){}fn f<T>(x:T)where for<'a>&'a T:D{need(x)}",
        false,
    );
}

#[test]
fn subject_does_not_extend_argument() {
    compare(
        "no_argument_escape",
        "trait D{} fn need(_: &'static u32){}fn f(x:&u32)where for<'a>&'a u32:D{need(x)}",
        false,
    );
}
