//! Compile-only rustc oracles: rejected programs must never be executed.
use std::process::Command;

#[test]
fn promotion_boundaries_match_rustc() {
    let dir = std::env::temp_dir().join(format!("flux-empty-mut-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cases = [
        (
            "default_nested",
            "fn f<'a>(_: &'a u8) -> &'a mut [&'a u8] { Default::default() }",
            true,
            true,
        ),
        (
            "default_lifetime",
            "fn f<'a>(_: &'a u8)-> &'static mut [&'a u8] { Default::default() } fn bad() { let x=0; let y=f(&x); let _=y.len(); }",
            false,
            false,
        ),
        (
            "const_empty",
            "const fn f<T>() -> &'static mut [T] { &mut [] } const A: &'static mut [u64] = f();",
            true,
            true,
        ),
        (
            "const_nonempty",
            "const A: &'static mut [u8] = &mut [0];",
            false,
            false,
        ),
        (
            "const_local",
            "const fn f() -> &'static mut [u8;0] { let mut x=[]; &mut x } const A: &'static mut [u8;0] = f();",
            false,
            false,
        ),
        (
            "nested_lifetime",
            "fn f<'a>(_: &'a u8) -> &'a mut [&'a u8] { &mut [] }",
            true,
            true,
        ),
        (
            "invariant",
            "fn f<'a>(x: &'a u8, target: &mut &'static mut [&'static u8]) { let a: &mut [&'a u8] = &mut []; *target = a; let _ = x; }",
            false,
            false,
        ),
        (
            "typed_mutation",
            "fn f() { let x: &mut [u8;0] = &mut []; let _: &mut [u16;0] = x; }",
            false,
            false,
        ),
        (
            "generic",
            "fn f<'a,T>() -> &'a mut [T] { &mut [] }",
            true,
            true,
        ),
        (
            "drop",
            "struct D; impl Drop for D { fn drop(&mut self) {} } fn f() -> &'static mut [D;0] { &mut [] }",
            true,
            true,
        ),
        (
            "cell",
            "fn f() -> &'static mut [std::cell::Cell<u8>] { &mut [] }",
            true,
            true,
        ),
        (
            "nonempty",
            "fn f() -> &'static mut [u8] { &mut [0] }",
            false,
            false,
        ),
        (
            "zst_nonempty",
            "fn f() -> &'static mut [();1] { &mut [()] }",
            false,
            false,
        ),
        (
            "unit",
            "fn f() -> &'static mut () { &mut () }",
            false,
            false,
        ),
        (
            "local",
            "fn f() -> &'static mut [u8;0] { let mut x=[]; &mut x }",
            false,
            false,
        ),
        (
            "alias",
            "fn f()->(&'static mut [u8;0], &'static mut [u8;0]) { let x=&mut []; (x,x) }",
            false,
            false,
        ),
        (
            "shared_local",
            "fn f()-> &'static mut [u8;0] { let x=&[]; &mut *x }",
            false,
            false,
        ),
        (
            "lifetime",
            "fn f<'a>(_: &'a u8)-> &'static mut [&'a u8] { &mut [] } fn bad() { let x=0; let y=f(&x); let _=y.len(); }",
            false,
            false,
        ),
        // Deliberately outside the literal-empty-array whitelist. Evaluating
        // repeat operands and general const expressions is separate work.
        (
            "repeat",
            "fn f()-> &'static mut [u8;0] { &mut [0;0] }",
            true,
            false,
        ),
    ];
    for (name, source, rustc_accepts, flux_accepts) in cases {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, format!("{source}\nfn main() {{}}\n")).unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
            .output()
            .unwrap();
        assert_eq!(
            rustc.status.success(),
            rustc_accepts,
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
            flux_accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}
