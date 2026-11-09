//! Owning Box calls consume the box while borrowed Fn calls retain it.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-box-call-{name}-{}", std::process::id()));
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
fn owned_dynamic() {
    compare(
        "owned_dynamic",
        r#"fn f(f:Box<dyn FnOnce()->u32>)->u32{f()}"#,
        true,
    );
}

#[test]
fn generic_consumer() {
    compare(
        "generic_consumer",
        r#"fn invoke<F:FnOnce()->u32>(f:F)->u32{f()} fn f(f:Box<dyn FnOnce()->u32>)->u32{invoke(f)}"#,
        true,
    );
}

#[test]
fn borrowed_capture() {
    compare(
        "borrowed_capture",
        r#"fn f<'a>(f:Box<dyn FnOnce()->u32+'a>)->u32{f()}"#,
        true,
    );
}

#[test]
fn use_after_call() {
    compare(
        "use_after_call",
        r#"fn f(f:Box<dyn FnOnce()->u32>){f();f();}"#,
        false,
    );
}

#[test]
fn shared_once() {
    compare(
        "shared_once",
        r#"fn f(f:&dyn FnOnce()->u32)->u32{f()}"#,
        false,
    );
}

#[test]
fn shared_box_once() {
    compare(
        "shared_box_once",
        r#"fn f(f:&Box<dyn FnOnce()->u32>)->u32{f()}"#,
        false,
    );
}

#[test]
fn mutable_box_once() {
    compare(
        "mutable_box_once",
        r#"fn f(f:&mut Box<dyn FnOnce()->u32>)->u32{f()}"#,
        false,
    );
}

#[test]
fn unsized_by_value() {
    compare(
        "unsized_by_value",
        r#"fn f(f:Box<dyn FnOnce()->u32>)->u32{(*f)()}"#,
        false,
    );
}

#[test]
fn borrowed_capture_escape() {
    compare(
        "borrowed_capture_escape",
        r#"fn f<'a>(f:Box<dyn FnOnce()->u32+'a>)->Box<dyn FnOnce()->u32>{f}"#,
        false,
    );
}

#[test]
fn repeated_shared() {
    compare(
        "repeated_shared",
        r#"fn f(f:Box<dyn Fn()->u32>)->u32{f()+f()}"#,
        true,
    );
}

#[test]
fn repeated_mutable() {
    compare(
        "repeated_mutable",
        r#"fn f(mut f:Box<dyn FnMut()->u32>)->u32{f()+f()}"#,
        true,
    );
}

#[test]
fn direct_owned_reuse_rejected() {
    compare(
        "direct_owned_reuse",
        r#"fn f(){ let s=String::from("move"); let f=Box::new(move || s); f(); f(); }"#,
        false,
    );
}

#[test]
fn direct_mutable_binding_required() {
    compare(
        "direct_mutable_binding",
        r#"fn f(){ let mut x=0; let f=Box::new(move || {x+=1; x}); f(); }"#,
        false,
    );
}
