//! Opaque auto traits follow hidden captures without exposing ordinary traits.

fn compare(name: &str, source: &str, accepted: bool) {
    let root = std::env::temp_dir().join(format!("flux-opaque-auto-{name}-{}", std::process::id()));
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
fn provider_later() {
    compare(
        "provider_later",
        r#"fn need<T:Send>(_:T){}fn caller<F:FnOnce()+Send>(f:F){need(adapt(f));}fn adapt<F:FnOnce()>(f:F)->impl FnOnce(u32){move |_|f()}"#,
        true,
    );
}

#[test]
fn provider_earlier() {
    compare(
        "provider_earlier",
        r#"fn adapt<F:FnOnce()>(f:F)->impl FnOnce(u32){move |_|f()}fn need<T:Send>(_:T){}fn caller<F:FnOnce()+Send>(f:F){need(adapt(f));}"#,
        true,
    );
}

#[test]
fn chained_provider() {
    compare(
        "chained_provider",
        r#"fn need<T:Send>(_:T){}fn caller<F:FnOnce()+Send>(f:F){need(adapt(f));}fn adapt<F:FnOnce()>(f:F)->impl FnOnce(u32){wrap(f)}fn wrap<F:FnOnce()>(f:F)->impl FnOnce(u32){move |_|f()}"#,
        true,
    );
}

#[test]
fn missing_send() {
    compare(
        "missing_send",
        r#"fn need<T:Send>(_:T){}fn caller<F:FnOnce()>(f:F){need(adapt(f));}fn adapt<F:FnOnce()>(f:F)->impl FnOnce(u32){move |_|f()}"#,
        false,
    );
}

#[test]
fn rc_capture() {
    compare(
        "rc_capture",
        r#"fn need<T:Send>(_:T){}fn caller(){need(adapt(std::rc::Rc::new(7)));}fn adapt(x:std::rc::Rc<u32>)->impl FnOnce()->u32{move ||*x}"#,
        false,
    );
}

#[test]
fn ordinary_trait_stays_abstract() {
    compare(
        "ordinary_trait_stays_abstract",
        r#"fn need<T:Copy>(_:T){}fn value()->impl Send{7u32}fn caller(){need(value());}"#,
        false,
    );
}

#[test]
fn borrowed_capture_is_not_static() {
    compare(
        "borrowed_capture_is_not_static",
        r#"fn need<T:Send+'static>(_:T){}fn caller(x:&u32){need(adapt(x));}fn adapt(x:&u32)->impl FnOnce()->u32+use<'_>{move||*x}"#,
        false,
    );
}
#[test]
fn mutual_opaque_cycle() {
    compare(
        "mutual_opaque_cycle",
        r#"fn first()->impl Send{second()}fn second()->impl Send{first()}fn need<T:Send>(_:T){}fn caller(){need(first());}"#,
        false,
    );
}

#[test]
fn hidden_auto_impl_static_condition() {
    compare(
        "hidden_auto_impl_static_condition",
        r#"struct Guard<T>(T);unsafe impl<T:'static>Send for Guard<T>{}fn adapt<T>(x:T)->impl Sized{Guard(x)}fn need<T:Send>(_:T){}fn caller(x:&u32){need(adapt(x));}"#,
        false,
    );
}
#[test]
fn recursive_function_closed_output() {
    compare(
        "recursive_function_closed_output",
        r#"fn value(n:u32)->impl Send{if n>0{drop(value(n-1));}7u32}fn need<T:Send>(_:T){}fn caller(){need(value(2));}"#,
        true,
    );
}

#[test]
fn borrowed_callback_auto_traits() {
    compare(
        "borrowed_callback_auto_traits",
        r#"fn adapt<F:Fn()>(f:F)->impl Fn(){move||f()}fn need<F:Send+Sync>(_:F){}fn call<F:Fn()+Sync>(f:F){need(adapt(&f));}"#,
        true,
    );
}
#[test]
fn captured_static_condition() {
    compare(
        "captured_static_condition",
        r#"struct Guard<T>(T);unsafe impl<T:'static>Send for Guard<T>{}fn adapt<T>(x:Guard<T>)->impl FnOnce(){move||drop(x)}fn need<T:Send>(_:T){}fn call(x:&u32){need(adapt(Guard(x)));}"#,
        false,
    );
}
