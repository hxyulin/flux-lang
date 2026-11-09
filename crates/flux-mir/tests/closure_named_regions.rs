//! Closure annotations and receiver captures preserve their enclosing contracts.

fn compare(name: &str, source: &str, accepted: bool) {
    let root =
        std::env::temp_dir().join(format!("flux-closure-named-{name}-{}", std::process::id()));
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
fn constructor_argument() {
    compare(
        "constructor",
        "fn f<'a>(x: &'a u32) -> &'a u32 { (|| Ok::<&'a u32, ()>(x))().unwrap() }",
        true,
    );
}

#[test]
fn constructor_wrong_owner() {
    compare(
        "wrong_owner",
        "fn f<'a, 'b>(x: &'b u32) -> &'b u32 { (|| Ok::<&'a u32, ()>(x))().unwrap() }",
        false,
    );
}

#[test]
fn constructor_local_escape() {
    compare(
        "local_escape",
        "fn f<'a>() -> &'a u32 { (|| { let x = 7; Ok::<&'a u32, ()>(&x) })().unwrap() }",
        false,
    );
}

#[test]
fn nested_constructor_argument() {
    compare(
        "nested",
        "fn f<'a>(x: &'a u32) -> &'a u32 { (|| (|| Ok::<&'a u32, ()>(x))())().unwrap() }",
        true,
    );
}

#[test]
fn nested_constructor_wrong_owner() {
    compare(
        "nested_wrong",
        "fn f<'a, 'b>(x: &'b u32) -> &'b u32 { (|| (|| Ok::<&'a u32, ()>(x))())().unwrap() }",
        false,
    );
}

#[test]
fn function_type_argument() {
    compare(
        "function_type",
        "fn id<T>(x: T) -> T { x } fn f<'a>(x: &'a u32) -> &'a u32 { (|| id::<&'a u32>(x))() }",
        true,
    );
}

#[test]
fn function_type_wrong_owner() {
    compare(
        "function_wrong",
        "fn id<T>(x: T) -> T { x } fn f<'a, 'b>(x: &'b u32) -> &'b u32 { (|| id::<&'a u32>(x))() }",
        false,
    );
}

#[test]
fn invariant_constructor_argument() {
    compare(
        "invariant",
        "fn f<'a>(x: &'a mut &'a u32) { (move || { let y = x; let _ = Some::<&'a mut &'a u32>(y); })(); }",
        true,
    );
}

#[test]
fn invariant_constructor_wrong_owner() {
    compare(
        "invariant_wrong",
        "fn f<'a, 'b>(x: &'b mut &'b u32) { (move || { let y = x; let _ = Some::<&'a mut &'a u32>(y); })(); }",
        false,
    );
}

#[test]
fn unused_annotation_still_checked() {
    compare(
        "unused",
        "fn f<'a, 'b>(x: &'b u32) { let _c = || { let _ = Ok::<&'a u32, ()>(x); }; }",
        false,
    );
}

#[test]
fn shared_and_mutable_receiver_capture() {
    compare(
        "receiver",
        "trait B { fn chunk(&self) -> &[u8]; fn bump(&mut self); fn go(&mut self) { (|| { if self.chunk().len() == 0 { self.bump(); } })(); } } impl<T: B + ?Sized> B for &mut T { fn chunk(&self) -> &[u8] { (**self).chunk() } fn bump(&mut self) { (**self).bump(); } }",
        true,
    );
}

#[test]
fn unique_receiver_capture_conflicts_with_shared_borrow() {
    compare(
        "receiver_conflict",
        "fn f(x: &mut u32) { let mut c = || { let _r = &x; *x += 1; }; let shared = &x; c(); let _ = shared; }",
        false,
    );
}

#[test]
fn unique_receiver_capture_reverse_use_order() {
    compare(
        "receiver_reverse",
        "fn f(x: &mut u32) { let mut c = || { *x += 1; let _r = &x; }; c(); }",
        true,
    );
}

#[test]
fn assigning_reference_still_requires_mutable_binding() {
    compare(
        "assign_reference",
        "fn f<'a>(x: &'a mut u32, y: &'a mut u32) { (|| { let _r = &x; x = y; })(); }",
        false,
    );
}

#[test]
fn boxed_receiver_still_requires_mutable_binding() {
    compare(
        "boxed_receiver",
        "fn f(x: Box<u32>) { (|| { let _r = &x; *x += 1; })(); }",
        false,
    );
}

#[test]
fn local_annotation_uses_enclosing_lifetime() {
    compare(
        "local_annotation",
        "fn f<'a>(x:&'a u32)->&'a u32{(||{let y:&'a u32=x;y})()}",
        true,
    );
}

#[test]
fn local_annotation_rejects_wrong_lifetime() {
    compare(
        "local_annotation_wrong",
        "fn f<'a,'b>(x:&'b u32)->&'b u32{(||{let y:&'a u32=x;y})()}",
        false,
    );
}

#[test]
fn local_annotation_rejects_local_escape() {
    compare(
        "local_annotation_escape",
        "fn f<'a>()->&'a u32{(||{let x=7;let y:&'a u32=&x;y})()}",
        false,
    );
}

#[test]
fn nested_local_annotation_uses_enclosing_lifetime() {
    compare(
        "nested_local_annotation",
        "fn f<'a>(x:&'a u32)->&'a u32{(||(||{let y:&'a u32=x;y})())()}",
        true,
    );
}
