//! Callable objects preserve input binders and captured regions.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!(
        "flux-callable-object-{name}-{}",
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
fn owned_nested_object() {
    compare(
        "owned_nested_object",
        r#"fn call(f:&dyn Fn(Box<dyn std::any::Any+Send>)){f(Box::new(7u32));}"#,
        true,
    );
}
#[test]
fn captured_input_lifetime() {
    compare(
        "captured_input_lifetime",
        r#"struct Scope<'s>(&'s u32);fn call<'s>(f:&dyn Fn(&Scope<'s>),x:&'s u32){f(&Scope(x));}"#,
        true,
    );
}
#[test]
fn wrong_captured_input() {
    compare(
        "wrong_captured_input",
        r#"struct Scope<'s>(&'s u32);fn call(f:&dyn Fn(&Scope<'static>),x:&u32){f(&Scope(x));}"#,
        false,
    );
}
#[test]
fn borrowed_environment() {
    compare(
        "borrowed_environment",
        r#"struct Scope<'s>(&'s u32);fn make<'s>(x:&'s u32)->Box<dyn Fn(&Scope<'s>)->u32+'s>{Box::new(move|value:&Scope<'s>|*x+*value.0)}"#,
        true,
    );
}
#[test]
fn binder_escape() {
    compare(
        "binder_escape",
        r#"struct Scope<'s>(&'s u32);fn make<'s>(slot:&'s mut Option<&'s Scope<'s>>)->Box<dyn FnMut(&Scope<'s>)+'s>{Box::new(move|value|*slot=Some(value))}"#,
        false,
    );
}

#[test]
fn static_alias_call() {
    compare(
        "static_alias_call",
        r#"type Input=&'static str;fn call(f:&dyn Fn(Input)){f("fixed");}"#,
        true,
    );
}
#[test]
fn static_alias_rejects_borrowed_input() {
    compare(
        "static_alias_rejects_borrowed_input",
        r#"type Input=&'static str;fn call(f:&dyn Fn(Input),x:&str){f(x);}"#,
        false,
    );
}
#[test]
fn repeated_alias_call() {
    compare(
        "repeated_alias_call",
        r#"type Input<'a>=(&'a str,&'a str);fn call<'a>(f:&dyn Fn(Input<'a>),x:&'a str){f((x,x));}"#,
        true,
    );
}
#[test]
fn repeated_alias_rejects_borrowed_input() {
    compare(
        "repeated_alias_rejects_borrowed_input",
        r#"type Input<'a>=(&'a str,&'a str);fn call(f:&dyn Fn(Input<'static>),x:&str){f((x,x));}"#,
        false,
    );
}
