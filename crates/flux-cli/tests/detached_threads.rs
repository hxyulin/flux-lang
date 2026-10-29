#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn detached_threads_match_rustc() {
    support::oracle::compare_fixture("detached_threads");
}

#[test]
fn detached_thread_ownership_matches_rustc() {
    use std::process::Command;
    let root = std::env::temp_dir().join(format!("flux-spawn-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expected, source) in [
        (
            "owned_result",
            true,
            "fn main(){let result:std::thread::Result<String>=std::thread::spawn(||String::from(\"owned\")).join();assert_eq!(result.unwrap(),\"owned\");}",
        ),
        (
            "owned_capture",
            true,
            "fn main(){let data=String::from(\"move\");let h=std::thread::spawn(move||data.len());assert_eq!(h.join().unwrap(),4);}",
        ),
        (
            "borrowed_capture",
            false,
            "fn main(){let data=String::from(\"short\");let h=std::thread::spawn(||data.len());let _=h.join();}",
        ),
        (
            "borrowed_result",
            false,
            "fn main(){let data=String::from(\"short\");let h=std::thread::spawn(move||data.as_str());let _=h.join();}",
        ),
        (
            "non_send_capture",
            false,
            "fn main(){let data=std::rc::Rc::new(3);let _=std::thread::spawn(move||drop(data));}",
        ),
        (
            "non_send_result",
            false,
            "fn main(){let _=std::thread::spawn(||std::rc::Rc::new(3));}",
        ),
        (
            "handle_shared_cell_result",
            true,
            "fn need<T:Send+Sync>(_:T){}fn main(){need(std::thread::spawn(||std::cell::Cell::new(3)));}",
        ),
        (
            "handle_not_clone",
            false,
            "fn main(){let h=std::thread::spawn(||3);let _=h.clone();}",
        ),
        (
            "consuming_join",
            false,
            "fn main(){let h=std::thread::spawn(||3);h.join();h.join();}",
        ),
        (
            "scoped_result_alias",
            true,
            "fn main(){let value=String::from(\"scope\");let result:std::thread::Result<&str>=std::thread::scope(|s|s.spawn(||value.as_str()).join());assert_eq!(result.unwrap(),\"scope\");}",
        ),
        (
            "error_any_send",
            true,
            "fn main(){let error:Box<dyn std::any::Any+Send>=Box::new(7u32);let _:std::thread::Result<()>=Err(error);}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            reference.status.success(),
            expected,
            "Rust {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text(&path, source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let accepted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(accepted, expected, "Flux {name}: {:?}", typed.diagnostics);
    }
}
