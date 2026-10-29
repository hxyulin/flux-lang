#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn mpsc_channels_match_rustc() {
    support::oracle::compare_fixture("mpsc_channels");
}

#[test]
fn channel_ownership_contracts_match_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-channel-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, accepted, body) in [
        (
            "endpoint_unwind_safe",
            true,
            "fn need<T:std::panic::UnwindSafe+std::panic::RefUnwindSafe>(_:T){}fn main(){need(channel::<std::cell::Cell<u32>>().0);need(sync_channel::<std::cell::Cell<u32>>(0).0);need(channel::<std::cell::Cell<u32>>().1);}",
        ),
        (
            "sender_shared_cell",
            true,
            "fn need<T:Send+Sync>(_:T){}fn main(){need(channel::<std::cell::Cell<u32>>().0);need(sync_channel::<std::cell::Cell<u32>>(1).0);}",
        ),
        (
            "receiver_movable",
            true,
            "fn need<T:Send>(_:T){}fn main(){need(channel::<std::cell::Cell<u32>>().1);}",
        ),
        (
            "receiver_not_sync",
            false,
            "fn need<T:Sync>(_:T){}fn main(){need(channel::<u32>().1);}",
        ),
        (
            "sender_not_send_rc",
            false,
            "fn need<T:Send>(_:T){}fn main(){need(channel::<std::rc::Rc<u32>>().0);}",
        ),
        (
            "sync_sender_not_send_rc",
            false,
            "fn need<T:Send>(_:T){}fn main(){need(sync_channel::<std::rc::Rc<u32>>(0).0);}",
        ),
        (
            "receiver_not_send_rc",
            false,
            "fn need<T:Send>(_:T){}fn main(){need(channel::<std::rc::Rc<u32>>().1);}",
        ),
        (
            "receiver_not_clone",
            false,
            "fn main(){let _=channel::<u32>().1.clone();}",
        ),
        (
            "sender_clone_no_payload_clone",
            true,
            "struct Value;fn main(){let (tx,_)=channel::<Value>();let _=tx.clone();}",
        ),
        (
            "errors_debug_without_payload_debug",
            true,
            "struct Value;fn main(){let e=SendError(Value);let _=format!(\"{:?}\",e);let _=format!(\"{:?}\",TrySendError::Full(Value));}",
        ),
        (
            "owned_iteration",
            true,
            "fn main(){let(tx,rx)=channel();tx.send(String::from(\"value\")).unwrap();drop(tx);for value in rx{assert_eq!(value,\"value\");}}",
        ),
        (
            "borrowed_iterator_not_send",
            false,
            "fn need<T:Send>(_:T){}fn main(){let(_,rx)=channel::<u32>();need(rx.iter());}",
        ),
        (
            "iterator_keeps_receiver",
            false,
            "fn main(){let(_,rx)=channel::<u32>();let mut it=rx.iter();drop(rx);it.next();}",
        ),
        (
            "owned_iter_send",
            true,
            "fn need<T:Send>(_:T){}fn main(){need(channel::<u32>().1.into_iter());}",
        ),
        (
            "iter_not_fused",
            false,
            "fn need<T:std::iter::FusedIterator>(_:T){}fn main(){let(_,rx)=channel::<u32>();need(rx.iter());}",
        ),
    ] {
        let source = format!("use std::sync::mpsc::*;{body}");
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, &source).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            accepted,
            "Rust {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let admitted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(admitted, accepted, "Flux {name}: {:?}", typed.diagnostics);
    }
}

#[test]
fn production_channel_panic_and_wakeup_invariants() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/channels-host/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let driver = root.join("driver.rs");
    let source = include_str!("fixtures/mpsc_channels/panic.rs.txt").replace(
        "__SOURCE__",
        repo.join("library/std/src/sync/mpsc.rs").to_str().unwrap(),
    );
    std::fs::write(&driver, source).unwrap();
    for optimized in [false, true] {
        let exe = root.join(format!("host-{optimized}"));
        let mut rustc = Command::new("rustc");
        rustc
            .args(["--edition=2024", "-Awarnings"])
            .arg(&driver)
            .arg("-o")
            .arg(&exe);
        if optimized {
            rustc.arg("-O");
        }
        let built =
            support::process::run(&mut rustc, &root, "build", Duration::from_secs(60)).completed();
        support::oracle::success(&built, "channel host build");
        let output = support::process::run(
            &mut Command::new(exe),
            &root,
            "run",
            Duration::from_secs(60),
        )
        .completed();
        support::oracle::success(&output, "channel host run");
        assert_eq!(output.stdout, b"channel panic complete\n");
    }
}
