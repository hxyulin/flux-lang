//! Associated output bounds elaborate into owner-checked projection predicates.

fn compare(name: &str, source: &str, accepted: bool) {
    let root =
        std::env::temp_dir().join(format!("flux-output-bound-{name}-{}", std::process::id()));
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
fn inline() {
    compare(
        "inline",
        r#"fn accept<I:IntoIterator<Item:Send>>(_:I){}fn caller(){accept(vec![7u32]);}"#,
        true,
    );
}
#[test]
fn where_clause() {
    compare(
        "where_clause",
        r#"fn accept<I>(_:I)where I:IntoIterator<Item:Send>{}fn caller(){accept(vec![7u32]);}"#,
        true,
    );
}
#[test]
fn reject_rc() {
    compare(
        "reject_rc",
        r#"fn accept<I:IntoIterator<Item:Send>>(_:I){}fn caller(){accept(vec![std::rc::Rc::new(7)]);}"#,
        false,
    );
}
#[test]
fn nested() {
    compare(
        "nested",
        r#"fn accept<I:IntoIterator<Item:IntoIterator<Item:Send>>>(_:I){}fn caller(){accept(vec![vec![7u32]]);}"#,
        true,
    );
}
#[test]
fn missing_owner() {
    compare(
        "missing_owner",
        r#"trait Source{type Item;}fn accept<T:Source<Item:Send>>(_:T){}fn caller(){accept(7u32);}"#,
        false,
    );
}
#[test]
fn missing_output() {
    compare(
        "missing_output",
        r#"trait Source{}fn accept<T:Source<Item:Send>>(_:T){}"#,
        false,
    );
}
#[test]
fn misplaced() {
    compare(
        "misplaced",
        r#"struct S;fn caller(){let _:S<Item:Send>;}"#,
        false,
    );
}

#[test]
fn inherited_output() {
    compare(
        "inherited_output",
        r#"fn require<I:DoubleEndedIterator<Item:Clone>>(_:I){}fn call(){require([7u32].into_iter());}"#,
        true,
    );
}
#[test]
fn named_owner_is_unambiguous() {
    compare(
        "named_owner_is_unambiguous",
        r#"trait A{type Item;}trait B{type Item;}fn require<T:A<Item:Send>+B>(){}"#,
        true,
    );
}
#[test]
fn repeated_bounds() {
    compare(
        "repeated_bounds",
        r#"fn require<I:Iterator<Item:Send,Item:Sync>>(_:I){}fn call(){require([7u32].into_iter());}"#,
        true,
    );
}
#[test]
fn mixed_equality_and_bound() {
    compare(
        "mixed_equality_and_bound",
        r#"fn require<I:Iterator<Item=u32,Item:Send>>(_:I){}fn call(){require([7u32].into_iter());}"#,
        true,
    );
}
#[test]
fn explicit_qualified_inherited_output_is_invalid() {
    compare(
        "explicit_qualified_inherited_output_is_invalid",
        r#"fn require<I:DoubleEndedIterator>(){let _:Option<<I as DoubleEndedIterator>::Item>;}"#,
        false,
    );
}

#[test]
fn empty_bound_cannot_hide_missing_output() {
    compare(
        "empty_bound_cannot_hide_missing_output",
        r#"trait Source{}fn accept<T:Source<Missing:>>(_:T){}"#,
        false,
    );
}
