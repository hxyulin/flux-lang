//! Const closure construction retains ordinary runtime bodies and capture checks.
mod support;

use std::process::Command;

#[test]
fn const_closure_contexts_match_rustc() {
    let root = std::env::temp_dir().join(format!("flux-const-closures-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source, accepts) in [
        (
            "runtime_body",
            "fn runtime()->i32{3} static F:fn()->i32=||runtime();",
            true,
        ),
        (
            "const_fn_body",
            "fn runtime()->i32{3} const fn f()->fn()->i32{||runtime()}",
            true,
        ),
        (
            "static_borrow",
            "static VALUE:i32=7;fn f(){let _=const{let x=&VALUE;move||*x};}",
            true,
        ),
        (
            "nested_borrowed_capture",
            "fn f(){let _=const{let x=3;Some(||x)};}",
            false,
        ),
        (
            "const_fn_captured",
            "const fn f(x:i32)->impl Fn()->i32{move||x}",
            true,
        ),
        (
            "bad_runtime_borrow",
            "static F:fn()->&'static i32=||{let x=3;&x};",
            false,
        ),
        ("outer_capture", "fn f(x:i32){let _=const{move||x};}", false),
        (
            "owned_capture",
            "fn f(){let _=const{let x=3;move||x};}",
            true,
        ),
        (
            "borrowed_capture",
            "fn f(){let _=const{let x=3;||x};}",
            false,
        ),
        (
            "pointer_capture",
            "const F:fn()->i32={let x=3;move||x};",
            false,
        ),
        ("const_fn_call", "const fn f()->i32{(||3)()}", false),
        (
            "const_generic_call",
            "const fn f<F:Fn()->i32>(f:F)->i32{f()}",
            false,
        ),
        ("const_call", "const X:i32=(||3)();", false),
        (
            "const_pointer_call",
            "const X:i32={let f:fn()->i32=||3;f()};",
            false,
        ),
        (
            "closure_inner_const",
            "fn runtime()->i32{3} static F:fn()->i32=||const{runtime()};",
            false,
        ),
        ("unsafe_body", "unsafe fn f(){} static F:fn()=||f();", false),
        (
            "static_capture",
            "use std::sync::LazyLock; static F:LazyLock<i32>={let x=3;LazyLock::new(move||x)};",
            false,
        ),
        (
            "nested",
            "static F:fn()->i32=||{let x=3;let f=move||x;f()};",
            true,
        ),
        (
            "no_sync",
            "use std::cell::Cell;use std::sync::LazyLock;static F:LazyLock<Cell<i32>>=LazyLock::new(||Cell::new(3));",
            false,
        ),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, format!("{source}\nfn main(){{}}")).unwrap();
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
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
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
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[cfg(all(target_arch = "aarch64", target_os = "macos"))]
fn const_closures_execute_like_rustc() {
    support::oracle::compare_fixture("const_closures");
}
