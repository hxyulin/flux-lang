//! Manual polling, pin projection and RawWaker ownership before async lowering.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn future_polling_matches_rustc() {
    support::oracle::compare_fixture("future_poll");
}

#[test]
fn pin_safety_rejections_match_rustc() {
    use std::process::Command;
    use std::time::Duration;
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-pin-errors"));
    for (name, source) in [
        (
            "escape",
            "fn escape()->core::pin::Pin<&'static mut u32>{core::pin::pin!(3)}fn main(){}",
        ),
        (
            "moved",
            "struct S;impl Drop for S{fn drop(&mut self){}}fn main(){let value=S;let _pin=core::pin::pin!(value);drop(value);}",
        ),
        (
            "unpin_constructor",
            "fn main(){let mut value=core::marker::PhantomPinned;let _=core::pin::Pin::new(&mut value);}",
        ),
        (
            "unpin_mut",
            "fn main(){let pinned=core::pin::pin!(core::marker::PhantomPinned);let _=pinned.get_mut();}",
        ),
        (
            "private",
            "fn main(){let mut value=3;let _=core::pin::Pin{pointer:&mut value};}",
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
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
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
