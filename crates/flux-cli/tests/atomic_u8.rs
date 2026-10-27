//! Exact byte storage and release/acquire publication.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn byte_atomics_match_rustc_off_and_on() {
    support::oracle::compare_fixture("atomic_u8");
}

#[test]
fn invalid_byte_atomic_orderings_panic() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/atomic-u8-orders/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, operation, order, message) in [
        (
            "load_release",
            "a.load(order);",
            "Release",
            "there is no such thing as a release load",
        ),
        (
            "load_acqrel",
            "a.load(order);",
            "AcqRel",
            "there is no such thing as an acquire-release load",
        ),
        (
            "store_acquire",
            "a.store(7,order);",
            "Acquire",
            "there is no such thing as an acquire store",
        ),
        (
            "store_acqrel",
            "a.store(7,order);",
            "AcqRel",
            "there is no such thing as an acquire-release store",
        ),
        (
            "cas_release",
            "let _=a.compare_exchange(0,1,Ordering::Relaxed,order);",
            "Release",
            "there is no such thing as a release failure ordering",
        ),
        (
            "cas_acqrel",
            "let _=a.compare_exchange(0,1,Ordering::SeqCst,order);",
            "AcqRel",
            "there is no such thing as an acquire-release failure ordering",
        ),
        (
            "weak_release",
            "let _=a.compare_exchange_weak(0,1,Ordering::Acquire,order);",
            "Release",
            "there is no such thing as a release failure ordering",
        ),
        (
            "weak_acqrel",
            "let _=a.compare_exchange_weak(0,1,Ordering::Release,order);",
            "AcqRel",
            "there is no such thing as an acquire-release failure ordering",
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(&source,format!("use core::sync::atomic::{{AtomicU8,Ordering}};#[inline(never)]fn bad(a:&AtomicU8,order:Ordering){{{operation}}}fn main(){{bad(&AtomicU8::new(0),Ordering::{order});}}")).unwrap();
        for mode in ["rustc", "flux", "flux-opt"] {
            let executable = directory.join(format!("{name}-{mode}"));
            let mut command = if mode == "rustc" {
                let mut command = Command::new("rustc");
                command.args(["--edition=2024", "-C", "panic=abort"]);
                command
            } else {
                let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
                command.arg("build");
                if mode == "flux-opt" {
                    command.arg("-O");
                }
                command
            };
            command.arg(&source).arg("-o").arg(&executable);
            let output = support::process::run(
                &mut command,
                &directory,
                &format!("build-{name}-{mode}"),
                Duration::from_secs(30),
            )
            .completed();
            support::oracle::success(&output, name);
            let output = support::process::run(
                Command::new(executable).env_remove("RUST_BACKTRACE"),
                &directory,
                &format!("run-{name}-{mode}"),
                Duration::from_secs(30),
            )
            .completed();
            assert!(!output.status.success(), "{name}-{mode} did not panic");
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(message),
                "{name}-{mode}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
