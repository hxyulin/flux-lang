//! Parsing accepts gated syntax without claiming its semantics are implemented.
use std::process::Command;

#[test]
fn nightly_syntax_matches_rustc_parse_only() {
    for (name, source) in [
        (
            "default_method",
            "impl<T> Trait for T { default fn f(&self) {} }",
        ),
        ("default_impl", "default unsafe impl<T> Trait for T {}"),
        (
            "qualified_default",
            "impl<T> Trait for T { default unsafe extern \"C\" fn f() {} }",
        ),
        (
            "box_patterns",
            "fn f(box x: Box<u8>) { match x { box (a | b) => {} } }",
        ),
        (
            "identifiers",
            "fn default() { let default = 0; let r#default = default; default!(); }",
        ),
    ] {
        let dir =
            std::env::temp_dir().join(format!("flux-nightly-parser-{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("probe.rs");
        std::fs::write(&path, source).unwrap();
        let rustc = Command::new("rustc")
            .env("RUSTC_BOOTSTRAP", "1")
            .args(["--edition=2024", "-Zparse-crate-root-only"])
            .arg(&path)
            .output()
            .unwrap();
        assert!(
            rustc.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let parse = flux_syntax::parse_source_file(source);
        assert!(parse.errors().is_empty(), "{name}: {:?}", parse.errors());
        assert_eq!(parse.root().text(), source);
        parse.tree().validate().unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }
}
