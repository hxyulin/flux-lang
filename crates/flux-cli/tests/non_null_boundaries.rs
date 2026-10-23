//! Compile-only oracles; rejected Rust is never executed.
use std::process::Command;

#[test]
fn reference_conversions_preserve_boundaries() {
    let dir = std::env::temp_dir().join(format!("flux-non-null-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        (
            "generic_shared",
            "fn f<T: ?Sized>(x: &T) -> NonNull<T> { NonNull::from(x) }",
            true,
        ),
        (
            "generic_mutable",
            "fn f<T: ?Sized>(x: &mut T) -> NonNull<T> { NonNull::from(x) }",
            true,
        ),
        (
            "const_len",
            "const fn f<T>(x: NonNull<[T]>) -> usize { x.len() }",
            true,
        ),
        (
            "raw_escape",
            "fn f() -> NonNull<u8> { let x = 1; NonNull::from(&x) }",
            true,
        ),
        (
            "raw_const",
            "fn f(x: *const u8) -> NonNull<u8> { NonNull::from(x) }",
            false,
        ),
        (
            "wrong_element",
            "fn f(x: &[u8]) -> NonNull<[u16]> { NonNull::from(x) }",
            false,
        ),
        (
            "wrong_mutability",
            "fn f(x: &u8) -> &'static mut u8 { x }",
            false,
        ),
        (
            "safe_read",
            "fn f(x: &u8) -> u8 { *NonNull::from(x).as_ptr() }",
            false,
        ),
        (
            "safe_as_ref",
            "fn f(x: &u8) -> &u8 { NonNull::from(x).as_ref() }",
            false,
        ),
        (
            "safe_as_mut",
            "fn f(x: &u8) -> &mut u8 { NonNull::from(x).as_mut() }",
            false,
        ),
        (
            "const_from",
            "const fn f(x: &u8) -> NonNull<u8> { NonNull::from(x) }",
            false,
        ),
        (
            "send",
            "fn send<T: Send>(_: T) {} fn f(x: &u8) { send(NonNull::from(x)); }",
            false,
        ),
        (
            "sync",
            "fn sync<T: Sync>(_: T) {} fn f(x: &u8) { sync(NonNull::from(x)); }",
            false,
        ),
        (
            "private",
            "fn f(x: &u8) { let _ = NonNull::from(x).pointer; }",
            false,
        ),
        (
            "reference_escape",
            "fn f() -> &'static u8 { let x = 1; let _ = NonNull::from(&x); &x }",
            false,
        ),
    ];
    for (name, source, accepts) in cases {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(
            &file,
            format!("use std::ptr::NonNull;\n{source}\nfn main() {{}}\n"),
        )
        .unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
}

#[test]
fn unchanged_dropless_source_clears_non_null_gaps() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = std::env::temp_dir().join(format!("flux-non-null-source-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let source = dir.join("probe.rs");
    std::fs::write(
        &source,
        format!(
            "#[path = {:?}] mod dropless;\nfn main() {{}}\n",
            root.join("crates/flux-arena/src/dropless.rs")
        ),
    )
    .unwrap();
    let rustc = Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(&source)
        .arg("-o")
        .arg(dir.join("probe.rmeta"))
        .output()
        .unwrap();
    assert!(
        rustc.status.success(),
        "{}",
        String::from_utf8_lossy(&rustc.stderr)
    );
    let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("mir")
        .arg(&source)
        .output()
        .unwrap();
    let errors = String::from_utf8_lossy(&flux.stderr);
    // The sibling pointer-alignment change may remove the remaining gap.
    // Until then it is the only permitted diagnostic from the actual source.
    assert!(
        errors.lines().all(
            |line| line.contains("the method `align_offset` on `*mut u8` is not supported yet")
        ),
        "{errors}"
    );
    assert!(
        flux.status.success() || !errors.is_empty(),
        "unexpected failure without diagnostics"
    );
}
