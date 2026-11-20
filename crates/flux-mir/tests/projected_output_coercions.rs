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
fn borrowed_wrapper_projection_checks_original_owner() {
    let source = include_str!("../../../tests/run/borrowed_wrapper_associated_signature.rs");
    let declarations = source.split_once("fn main()").unwrap().0;
    compare("borrowed_wrapper_associated_signature", declarations, true);
    compare(
        "borrowed_wrapper_static_owner",
        &declarations.replace(
            "impl<T: Object> Object for Forward<'_, T>",
            "impl<T: Object + 'static> Object for Forward<'static, T>",
        ),
        false,
    );
}

#[test]
fn nominal_field_projection_keeps_object_lifetime() {
    let source = r#"
        use std::sync::Weak;
        trait Array { type Item; }
        impl<T, const N: usize> Array for [T; N] { type Item = T; }
        struct Small<A: Array>(Vec<A::Item>);
        struct Buffer<'a>(Small<[Box<dyn FnOnce() + 'a>; 1]>);
        struct Resources<T>(Vec<Weak<T>>);
        impl<T> Resources<T> {
            fn iter(&self) -> impl Iterator<Item = &Weak<T>> + '_ { self.0.iter() }
        }
    "#;
    for (name, bound, accepted) in [
        ("nominal_static_callback", "'static", true),
        ("nominal_borrowed_callback", "'a", false),
    ] {
        compare(
            name,
            &format!(
                "{source}
                 fn requires_static(_: &Resources<Buffer<'static>>) {{}}
                 fn buffers<'a, 'b>(resources: &'b Resources<Buffer<{bound}>>)
                     -> impl Iterator<Item = &'b Weak<Buffer<{bound}>>> + 'b
                 {{
                     requires_static(resources);
                     resources.iter()
                 }}"
            ),
            accepted,
        );
    }
}

#[test]
fn moving_out_of_temporary_array_still_rejected() {
    compare(
        "indexed_move",
        r#"
        fn make(value: &mut i32) -> [&mut i32; 1] { [value] }
        fn inspect(value: &mut i32, index: usize) {
            let moved = make(value)[index];
            *moved += 1;
        }
        "#,
        false,
    );
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

#[test]
fn trait_method_output_can_unsize_after_selection() {
    compare(
        "trait_method_output_unsize",
        r#"struct X;
           trait Get<T> { fn get(&self) -> T; }
           impl Get<Box<u32>> for X { fn get(&self) -> Box<u32> { Box::new(42) } }
           fn f() -> Box<dyn std::fmt::Display> { X.get() }"#,
        true,
    );
}
