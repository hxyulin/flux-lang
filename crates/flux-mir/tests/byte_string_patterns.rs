//! Byte literal patterns match byte values and preserve array/slice domains.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;

#[test]
fn byte_string_patterns_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-byte-patterns-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepted) in [
        ("slice", "fn f(x:&[u8])->bool{matches!(x,b\"ab\")}", true),
        ("array", "fn f(x:&[u8;2])->bool{matches!(x,b\"ab\")}", true),
        (
            "inferred_element",
            "fn f(){let x=&[][..];match x{b\"ab\"=>(),_=>()}}",
            true,
        ),
        (
            "mutable_slice",
            "fn f(x:&mut[u8]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "mutable_array",
            "fn f(x:&mut[u8;2]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "double_reference",
            "fn f(x:&&[u8]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "bare_slice",
            "fn f(x:&[u8]){match *x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "bare_array",
            "fn f(x:[u8;2]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "wrong_length",
            "fn f(x:&[u8;3]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "wrong_element",
            "fn f(x:&[i8]){match x{b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "nonexhaustive_slice",
            "fn f(x:&[u8]){match x{b\"ab\"=>()}}",
            false,
        ),
        (
            "nonexhaustive_array",
            "fn f(x:&[u8;2]){match x{b\"ab\"=>()}}",
            false,
        ),
        ("empty_array", "fn f(){let b\"\"=b\"\";}", true),
        (
            "empty_slice_refutable",
            "fn f(x:&[u8]){let b\"\"=x;}",
            false,
        ),
        ("nonempty_let", "fn f(){let b\"ab\"=b\"cd\";}", false),
        (
            "empty_array_exhaustive",
            "fn f(x:&[u8;0]){match x{b\"\"=>()}}",
            true,
        ),
        (
            "array_with_slice_coverage",
            "fn f(x:&[u8;1]){match x{b\"a\"=>(),[0..=255]=>()}}",
            true,
        ),
        (
            "array_complete_complement",
            "fn f(x:&[u8;1]){match x{b\"a\"=>(),&[0..=96]|&[98..=255]=>()}}",
            true,
        ),
        (
            "empty_uninitialized",
            "fn f(){let x:&[u8;0];let b\"\"=x;}",
            true,
        ),
        (
            "uninitialized_bytes",
            "fn f(){let x:&[u8;1];match x{b\"a\"=>(),_=>()}}",
            false,
        ),
        (
            "different_length_or",
            "fn f(x:&[u8]){match x{b\"a\"|b\"ab\"=>(),_=>()}}",
            true,
        ),
        (
            "array_different_length_or",
            "fn f(x:&[u8;1]){match x{b\"a\"|b\"ab\"=>(),_=>()}}",
            false,
        ),
        (
            "nested_tuple",
            "fn f(x:(&[u8],bool)){match x{(b\"a\",true)|(b\"b\",false)=>(),_=>()}}",
            true,
        ),
        (
            "match_ergonomics",
            "fn f(x:&Option<&[u8]>){match x{Some(b\"a\")=>(),_=>()}}",
            true,
        ),
        (
            "mutable_nested_ergonomics",
            "fn f(x:&mut Option<&[u8]>){match x{Some(b\"a\")=>(),_=>()}}",
            true,
        ),
        (
            "raw_and_escaped",
            "fn f(x:&[u8]){match x{br#\"a\\n\"#|b\"\\x00\\xff\\\"\\\\\"=>(),_=>()}}",
            true,
        ),
        (
            "const_match",
            "const fn f(x:&[u8])->u32{match x{b\"a\"=>1,b\"\"=>0,_=>2}}const C:u32=f(b\"a\");",
            true,
        ),
        (
            "borrowed_binding",
            "fn f<'a>(x:&'a[u8])->Option<&'a[u8]>{match x{y@b\"a\"=>Some(y),_=>None}}",
            true,
        ),
        (
            "binding_escape",
            "fn f(x:&[u8])->Option<&'static[u8]>{match x{y@b\"a\"=>Some(y),_=>None}}",
            false,
        ),
        ("c_string", "fn f(){match c\"a\"{c\"a\"=>(),_=>()}}", false),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let text = format!("{source}fn main(){{}}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &text).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text(&path, &text);
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
        assert_eq!(errors.is_empty(), accepted, "flux {name}: {errors:?}");
    }
    std::fs::remove_dir_all(root).unwrap();
}
