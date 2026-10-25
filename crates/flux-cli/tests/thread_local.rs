//! Native callbacks and TLS are compared with independently compiled Rust.
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    let stdout = directory.join(format!("{label}.stdout"));
    let stderr = directory.join(format!("{label}.stderr"));
    let mut child = command.stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap())).spawn().unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap(); child.wait().unwrap(); panic!("{label} exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output { status, stdout: std::fs::read(stdout).unwrap(), stderr: std::fs::read(stderr).unwrap() }
}

#[test]
fn callback_abi_and_tls_lifetimes_match_rustc() {
    let directory = std::env::temp_dir().join(format!("flux-tls-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, accepts) in [
        ("c_item", "extern \"C\" fn f(x:u32)->u32{x} fn check(){let p:extern \"C\" fn(u32)->u32=f;assert_eq!(p(3),3);}", true),
        ("default_c", "extern fn f(x:u32)->u32{x} fn check(){let _:extern \"C\" fn(u32)->u32=f;}", true),
        ("explicit_rust", "extern \"Rust\" fn f(){} fn check(){let _:fn()=f;}", true),
        ("c_to_rust", "extern \"C\" fn f(){} fn check(){let _:fn()=f;}", false),
        ("rust_to_c", "fn f(){} fn check(){let _:extern \"C\" fn()=f;}", false),
        ("closure_to_c", "fn check(){let _:extern \"C\" fn()=||();}", false),
        ("cast_c_to_rust", "extern \"C\" fn f(){} fn check(){let _=f as fn();}", false),
        ("unsafe_c", "unsafe extern \"C\" fn f(){} fn check(){let _:unsafe extern \"C\" fn()=f;}", true),
        ("unsafe_to_safe", "unsafe extern \"C\" fn f(){} fn check(){let _:extern \"C\" fn()=f;}", false),
        ("safe_to_unsafe", "extern \"C\" fn f(){} fn check(){let _:unsafe extern \"C\" fn()=f;}", true),
        ("foreign_item", "unsafe extern \"C\"{fn getpid()->i32;} fn check(){let _:unsafe extern \"C\" fn()->i32=getpid;}", true),
        ("c_fn_trait", "extern \"C\" fn c(){} fn invoke<F:Fn()>(f:F){f();} fn check(){invoke(c);}", false),
        ("c_pointer_fn_trait", "extern \"C\" fn c(){} fn invoke<F:Fn()>(f:F){f();} fn check(){let p:extern \"C\" fn()=c;invoke(p);}", false),
        ("c_dyn_fn", "extern \"C\" fn c(){} fn check(){let _: &dyn Fn()=&c;}", false),
        ("abi_identity", "trait Mark{} impl Mark for fn(){} impl Mark for extern \"C\" fn(){}", true),
        ("mismatched_trait_method", "trait T{extern \"C\" fn f();} impl T for (){fn f(){}}", false),
        ("track_caller", "#[track_caller] extern \"C\" fn f(){}", false),
        ("foreign_nested_extern", "unsafe extern \"C\"{extern \"C\" fn f();}", false),
        ("borrow_escape", "thread_local!{static V:std::cell::Cell<u32>=const{std::cell::Cell::new(1)};} fn check()->&'static std::cell::Cell<u32>{V.with(|v|v)}", false),
        ("try_borrow_escape", "thread_local!{static V:u32=1;} fn check()->&'static u32{V.try_with(|v|v).unwrap()}", false),
        ("non_sync_payload", "thread_local!{static V:std::cell::Cell<u32>=std::cell::Cell::new(1);} fn check(){V.with(|v|v.set(3));}", true),
        ("qualified_macro", "std::thread_local!{static V:u32=1;} fn check(){V.with(|v|assert_eq!(*v,1));}", true),
        ("imported_macro", "use std::thread_local;thread_local!{static V:u32=1;}", true),
        ("aliased_macro", "use std::thread_local as tls;tls!{static V:u32=1;}", true),
        ("nested_macro", "mod inside{use std::thread_local as tls;tls!{pub static V:u32=1;}}", true),
        ("no_std_omits_macro", "#![no_std] extern crate std; thread_local!{static V:u32=1;}", false),
        ("no_std_explicit_macro", "#![no_std] extern crate std; std::thread_local!{static V:u32=1;}", true),
    ] {
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, format!("{body}\nfn main(){{}}\n")).unwrap();
        let rustc = bounded(Command::new("rustc").args(["--edition=2024", "--emit=metadata", "-o"])
            .arg(directory.join(format!("{name}.rmeta"))).arg(&path), &directory, &format!("rustc-{name}"));
        assert_eq!(rustc.status.success(), accepts, "rustc {name}: {}", String::from_utf8_lossy(&rustc.stderr));
        let flux = bounded(Command::new(env!("CARGO_BIN_EXE_flux")).arg("mir").arg(&path), &directory, &format!("flux-{name}"));
        assert_eq!(flux.status.success(), accepts, "flux {name}: {}", String::from_utf8_lossy(&flux.stderr));
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn native_threads_have_isolated_values_and_run_destructors() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!("flux-tls-native-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = repository.join("tests/run/thread_local.rs");
    let reference = directory.join("rustc");
    let compiled = bounded(Command::new("rustc").args(["--edition=2024", "-C", "panic=abort", "-C", "overflow-checks=off"])
        .arg(&source).arg("-o").arg(&reference), &directory, "compile-rustc");
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let expected = bounded(&mut Command::new(&reference), &directory, "run-rustc");
    assert!(expected.status.success());
    assert_eq!(expected.stdout, b"tls 8 8 37 19\n");
    for optimized in [false, true] {
        let executable = directory.join(format!("flux-{optimized}"));
        let mut compile = Command::new(env!("CARGO_BIN_EXE_flux"));
        compile.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized { compile.arg("-O"); }
        let compiled = bounded(&mut compile, &directory, &format!("compile-flux-{optimized}"));
        assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
        let actual = bounded(&mut Command::new(&executable), &directory, &format!("run-flux-{optimized}"));
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
