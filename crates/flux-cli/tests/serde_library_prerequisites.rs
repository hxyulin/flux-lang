//! Real containers and owned C strings needed by Serde's standard surface.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
#[test]
fn serde_library_prerequisites_match_rustc() {
    support::oracle::compare_fixture("serde_library_prerequisites");
}

#[test]
fn collection_panic_invariants() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = repo.join(format!("out/serde-collection-panic/{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    // This host proof runs the production node/heap bodies with Rust's allocator
    // and unwinding. Removing only the trusted may_dangle marker makes the copy
    // compile on stable; drop-check admission is covered separately by Flux.
    let list =
        std::fs::read_to_string(repo.join("library/alloc/src/collections/linked_list.rs")).unwrap();
    assert_eq!(list.matches("#[may_dangle]").count(), 1);
    let marker = list.find("#[may_dangle]").unwrap();
    let header = list[..marker].rfind("unsafe impl").unwrap();
    let mut list = list.replace("#[may_dangle]", "");
    list.replace_range(header..header + "unsafe ".len(), "");
    std::fs::write(directory.join("linked_list.rs"), list).unwrap();
    std::fs::copy(
        repo.join("library/alloc/src/collections/binary_heap.rs"),
        directory.join("binary_heap.rs"),
    )
    .unwrap();
    std::fs::copy(
        repo.join("library/std/src/collections/vec_deque.rs"),
        directory.join("vec_deque.rs"),
    )
    .unwrap();
    std::fs::write(
        directory.join("probe.rs"),
        include_str!("fixtures/serde_library/panic.rs.txt"),
    )
    .unwrap();
    for optimized in [false, true] {
        let executable = directory.join(format!("probe-{optimized}"));
        let mut command = Command::new("rustc");
        command
            .args(["--edition=2024"])
            .arg(directory.join("probe.rs"))
            .arg("-o")
            .arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let output = support::process::run(
            &mut command,
            &directory,
            &format!("build-{optimized}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "host collection bodies");
        let output = support::process::run(
            &mut Command::new(executable),
            &directory,
            &format!("run-{optimized}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "collection panic invariants");
        assert_eq!(output.stdout, b"collection panic invariants passed\n");
    }
}

#[test]
fn collection_ownership_and_marker_contracts_match_rustc() {
    use std::path::Path;
    use std::process::Command;
    use std::time::Duration;
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/serde-collection-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, body, accepts) in [
        (
            "list_reference_drop",
            "fn main(){let mut list=LinkedList::new();let data=vec![7];list.push_back(&data[0]);assert_eq!(**list.front().unwrap(),7);}",
            true,
        ),
        (
            "list_owned_destructor",
            "struct Inspect<'a>(&'a u32);impl Drop for Inspect<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let mut list=LinkedList::new();let data=vec![7];list.push_back(Inspect(&data[0]));}",
            false,
        ),
        (
            "list_send",
            "fn need<T:Send>(_:T){}fn main(){let list:LinkedList<_>=[1,2].into();need(list);}",
            true,
        ),
        (
            "list_rc_not_send",
            "fn need<T:Send>(_:T){}fn main(){let list:LinkedList<_>=[std::rc::Rc::new(1)].into();need(list);}",
            false,
        ),
        (
            "iter_send",
            "fn need<T:Send>(_:T){}fn main(){let list:LinkedList<_>=[1,2].into();need(list.iter());}",
            true,
        ),
        (
            "iter_cell_not_send",
            "fn need<T:Send>(_:T){}fn main(){let list:LinkedList<_>=[std::cell::Cell::new(1)].into();need(list.iter());}",
            false,
        ),
        (
            "iter_mut_cell_send",
            "fn need<T:Send>(_:T){}fn main(){let mut list:LinkedList<_>=[std::cell::Cell::new(1)].into();need(list.iter_mut());}",
            true,
        ),
        (
            "list_reference_escape",
            "fn bad()->&'static u32{let list:LinkedList<_>=[1,2].into();list.front().unwrap()}fn main(){}",
            false,
        ),
        (
            "list_mut_iterator_conflict",
            "fn main(){let mut list:LinkedList<_>=[1,2].into();let mut a=list.iter_mut();let mut b=list.iter_mut();*a.next().unwrap()+=*b.next().unwrap();}",
            false,
        ),
        (
            "cstring_unsafe_raw",
            "fn main(){let value=CString::new(\"x\").unwrap();let pointer=value.into_raw();let _=CString::from_raw(pointer);}",
            false,
        ),
        (
            "heap_requires_ord",
            "struct NoOrd;fn main(){let mut heap=BinaryHeap::new();heap.push(NoOrd);}",
            false,
        ),
    ] {
        let source = directory.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("use std::collections::{{LinkedList,BinaryHeap}};use std::ffi::CString;{body}"),
        )
        .unwrap();
        let output = support::process::run(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata"])
                .arg(&source)
                .arg("-o")
                .arg(directory.join(format!("{name}.rmeta"))),
            &directory,
            &format!("rustc-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            output.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("mir")
                .arg(&source),
            &directory,
            &format!("flux-{name}"),
            Duration::from_secs(30),
        )
        .completed();
        assert_eq!(
            output.status.success(),
            accepts,
            "Flux {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
