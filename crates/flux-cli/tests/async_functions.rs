//! Async function futures retain arguments, input regions and output contracts.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

#[test]
fn async_functions_match_rustc() {
    support::oracle::compare_fixture("async_functions");
}

#[test]
fn async_function_rejections_match_rustc() {
    use std::{process::Command, time::Duration};
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-async-function-errors"));
    let cases = [
        (
            "return_local",
            "async fn f()->&'static u32{let x=7;&x}fn main(){}",
        ),
        (
            "unused_input_escape",
            "async fn f(_: &u32){}fn main(){let future;{let x=7;future=f(&x);}drop(future);}",
        ),
        (
            "mutable_capture",
            "async fn f(x:&mut u32){*x+=1;}fn main(){let mut x=7;let future=f(&mut x);x+=1;drop(future);}",
        ),
        (
            "moved_argument",
            "async fn f(_:String){}fn main(){let x=String::from(\"x\");let future=f(x);drop(x);drop(future);}",
        ),
        (
            "wrong_output",
            "async fn f()->u8{String::from(\"x\")}fn main(){}",
        ),
        (
            "missing_output_lifetime",
            "async fn f(x:&u32,y:&u32)->&u32{x}fn main(){}",
        ),
        ("unsafe_call", "async unsafe fn f(){}fn main(){let _=f();}"),
        (
            "nested_closure",
            "async fn f(){let _=||{async{}.await;};}fn main(){}",
        ),
        ("const_async", "const async fn f(){}fn main(){}"),
        (
            "foreign_async",
            "unsafe extern \"C\"{async fn f();}fn main(){}",
        ),
        (
            "dyn_async_trait",
            "trait T{async fn f(&self);}fn f(_: &dyn T){}fn main(){}",
        ),
        ("async_main", "async fn main(){}"),
        (
            "wrong_trait_output",
            "trait T{async fn f()->u32;}struct S;impl T for S{async fn f()->u8{7}}fn main(){}",
        ),
        ("unsized_output", "async fn f()->str{loop{}}fn main(){}"),
        (
            "trait_borrow_escape",
            "trait T{async fn f<'a>(&'a self)->&'static u32;}struct S(u32);impl T for S{async fn f<'a>(&'a self)->&'static u32{&self.0}}fn main(){}",
        ),
        (
            "refutable_parameter",
            "async fn f(Some(x):Option<u32>)->u32{x}fn main(){}",
        ),
        ("recursive", "async fn f(){f().await;}fn main(){}"),
        (
            "mutual_recursion",
            "async fn f(){g().await;}async fn g(){f().await;}fn main(){}",
        ),
        (
            "stored_recursive_future",
            "async fn f(){let next=f();core::future::ready(()).await;drop(next);}fn main(){}",
        ),
        (
            "trait_recursion",
            "trait T{async fn f(&self);}struct S;impl T for S{async fn f(&self){self.f().await;}}fn main(){}",
        ),
        (
            "not_unpin",
            "async fn f(){}fn need<T:Unpin>(_:T){}fn main(){need(f());}",
        ),
    ];
    for (name, text) in cases {
        let source = scratch.path().join(format!("{name}.rs"));
        std::fs::write(&source, text).unwrap();
        for (label, mut command) in [
            ("rustc", {
                let mut c = Command::new("rustc");
                c.args(["--edition=2024", "--emit=metadata"]);
                c
            }),
            ("flux", {
                let mut c = Command::new(env!("CARGO_BIN_EXE_fluxc"));
                c.arg("build");
                c
            }),
        ] {
            let output = support::process::run(
                command
                    .arg(&source)
                    .arg("-o")
                    .arg(scratch.path().join(format!("{name}-{label}"))),
                scratch.path(),
                &format!("{name}-{label}"),
                Duration::from_secs(30),
            )
            .completed();
            assert!(!output.status.success(), "{label} accepted {name}");
        }
    }
}

#[test]
fn async_inputs_are_captured_in_edition_2018() {
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-async-2018"));
    for (name, text, accepted) in [
        (
            "unused_escape",
            "async fn f(_: &u32){}fn main(){let future;{let x=7;future=f(&x);}drop(future);}",
            false,
        ),
        (
            "borrowed_output",
            "async fn f(x:&u32)->&u32{x}fn main(){}",
            true,
        ),
        (
            "trait_method",
            "trait T{async fn f(&self)->u32;}struct S;impl T for S{async fn f(&self)->u32{7}}fn main(){}",
            true,
        ),
    ] {
        let path = scratch.path().join(format!("{name}.rs"));
        std::fs::write(&path, text).unwrap();
        let oracle = std::process::Command::new("rustc")
            .args(["--edition=2018", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(scratch.path().join("probe.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            oracle.status.success(),
            accepted,
            "rustc {name}: {}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let mut db = flux_hir::SourceDb::new();
        let root = db.add_with_edition(&path, text, flux_syntax::Edition::E2018);
        let hir = flux_hir::lower(&mut db, root, flux_intern::Symbol::intern("async_2018"));
        let typed = flux_mir::check_file(&hir);
        let mir = flux_mir::build_file(&hir, &typed);
        assert_eq!(
            hir.diagnostics.is_empty()
                && typed.diagnostics.is_empty()
                && mir.diagnostics.is_empty(),
            accepted,
            "Flux {name}: {:?} {:?} {:?}",
            hir.diagnostics,
            typed.diagnostics,
            mir.diagnostics
        );
    }
}
