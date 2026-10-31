//! Stored associated types contribute to a reference's implied outlives bounds.
#[path = "../../../tests/support/cases.rs"]
#[allow(dead_code, reason = "oracle uses case selection only")]
mod case_filter;
use std::process::Command;

#[test]
fn stored_projection_and_cow_lifetimes_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-cow-regions-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let declarations = "use std::borrow::Cow;
        trait Value { type Stored; }
        struct Storage<T: Value>(T::Stored);
        struct Nested<T: Value>(Box<Storage<T>>);
        struct Borrowed<'a>(&'a u32);
        impl<'a> Value for Borrowed<'a> { type Stored = &'a u32; }
        struct Recursive<T: Value>(Option<Box<Recursive<T>>>, T::Stored);";
    for (name, body, accepts) in [
        (
            "normalized_owned_static",
            "fn need<T:'static>(_:T){} fn f(){ let x:Cow<'static,str>=Cow::Owned(String::from(\"x\")); need(x); }",
            true,
        ),
        (
            "generic_stored_static",
            "fn need<T:'static>(_:T){} fn f<T:Value+'static>(x:Storage<T>){ need(x); }",
            true,
        ),
        (
            "generic_stored_without_bound",
            "fn need<T:'static>(_:T){} fn f<T:Value>(x:Storage<T>){ need(x); }",
            false,
        ),
        (
            "stored_short_input",
            "fn need<T:'static>(_:T){} fn f<'a>(x:Storage<Borrowed<'a>>){ need(x); }",
            false,
        ),
        (
            "projection",
            "fn get<T:Value>(s:&Storage<T>)->&T::Stored { &s.0 }",
            true,
        ),
        (
            "nested",
            "fn get<T:Value>(s:&Nested<T>)->&T::Stored { &s.0.0 }",
            true,
        ),
        (
            "recursive",
            "fn get<T:Value>(s:&Recursive<T>)->&T::Stored { &s.1 }",
            true,
        ),
        (
            "projection_escape",
            "fn get<T:Value>(s:&Storage<T>)->&'static T::Stored { &s.0 }",
            false,
        ),
        (
            "nested_escape",
            "fn get<T:Value>(s:&Nested<T>)->&'static T::Stored { &s.0.0 }",
            false,
        ),
        (
            "stored_escape",
            "fn get()->Storage<Borrowed<'static>> { let x=2; Storage(&x) }",
            false,
        ),
        (
            "cow_borrow",
            "fn get<'a>(s:&'a str)->Cow<'a,str> { Cow::Borrowed(s) }",
            true,
        ),
        (
            "cow_escape",
            "fn get()->Cow<'static,str> { let s=String::from(\"local\"); Cow::Borrowed(&s) }",
            false,
        ),
        (
            "owned_borrow_escape",
            "fn get()->Cow<'static,[&'static u32]> { let x=2; Cow::Owned(vec![&x]) }",
            false,
        ),
        (
            "cow_owned",
            "fn get()->Cow<'static,str> { Cow::Owned(String::from(\"owned\")) }",
            true,
        ),
    ] {
        if !case_filter::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, format!("{declarations}\n{body}\nfn main() {{}}")).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata", "-o"])
            .arg(root.join(format!("{name}.rmeta")))
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .arg("mir")
            .arg(&path)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
        if !accepts {
            let errors = String::from_utf8_lossy(&flux.stderr);
            assert!(
                errors.contains("lifetime")
                    || errors.contains("may not live long enough")
                    || errors.contains("escapes outside")
                    || errors.contains("does not live long enough")
                    || errors.contains("cannot return"),
                "{name}: {errors}"
            );
        }
    }
}
