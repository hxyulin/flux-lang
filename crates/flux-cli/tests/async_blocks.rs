//! Async blocks preserve locals and resume rather than replaying their body.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn async_blocks_match_rustc() {
    support::oracle::compare_fixture("async_blocks");
}

#[test]
fn async_safety_rejections_match_rustc() {
    use std::{process::Command, time::Duration};
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-async-errors"));
    for (name, source) in [
        ("await_outside", "fn main(){async{}.await;}"),
        (
            "borrow_escape",
            "fn main(){let f;{let value=7u32;f=async{value+1};}drop(f);}",
        ),
        (
            "return_local",
            "fn f()->impl core::future::Future<Output=&'static u32>{async{let value=7u32;&value}}fn main(){}",
        ),
        ("unpin", "fn need<T:Unpin>(_:T){}fn main(){need(async{});}"),
        (
            "capture_mut",
            "fn main(){let mut value=1u32;let f=async{value+=1;};value+=1;drop(f);}",
        ),
        (
            "capture_move",
            "fn main(){let value=String::from(\"value\");let f=async move{drop(value);};drop(value);drop(f);}",
        ),
        (
            "output_escape",
            "use core::future::Future;use core::task::{Context,Poll,Waker};fn main(){let result;{let value=7u32;let mut f=core::pin::pin!(async{&value});let mut cx=Context::from_waker(Waker::noop());result=match f.as_mut().poll(&mut cx){Poll::Ready(value)=>value,Poll::Pending=>panic!()};}assert_eq!(*result,7);}",
        ),
    ] {
        let path = scratch.path().join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let oracle = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&path)
                .arg("-o")
                .arg(scratch.path().join("probe.rmeta")),
            scratch.path(),
            &format!("rustc-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        let actual = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .arg("mir")
                .arg(&path),
            scratch.path(),
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert!(!oracle.status.success(), "invalid rustc probe {name}");
        assert!(!actual.status.success(), "invalid Flux probe {name}");
    }
}
