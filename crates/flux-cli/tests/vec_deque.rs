//! Canonical ring ownership, drop checking and native parity.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(60)).completed()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn ring_matches_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/vec-deque/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("consumer.rs");
    std::fs::write(&source, include_str!("fixtures/vec_deque/consumer.rs.txt")).unwrap();
    let reference = root.join("reference");
    success(&run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "rustc-build",
    ));
    let expected = run(&mut Command::new(reference), &root, "reference-run");
    success(&expected);
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_flux"));
        cmd.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            cmd.arg("-O");
        }
        success(&run(&mut cmd, &root, &format!("build-{optimized}")));
        let actual = run(
            &mut Command::new(executable),
            &root,
            &format!("run-{optimized}"),
        );
        success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}
#[test]
fn ring_trait_and_drop_contracts_match_rustc() {
    let cases = [
        (
            "expired_reference",
            true,
            "fn main(){let mut q=VecDeque::new();let data=vec![7];q.push_back(&data[0]);assert_eq!(**q.front().unwrap(),7);}",
        ),
        (
            "inspecting_drop",
            false,
            "struct Inspect<'a>(&'a u32);impl Drop for Inspect<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let mut q=VecDeque::new();let data=vec![7];q.push_back(Inspect(&data[0]));}",
        ),
        (
            "owned_iter_expired",
            true,
            "fn main(){let mut it;let data=vec![7];let mut q=VecDeque::new();q.push_back(&data[0]);q.push_back(&data[0]);it=q.into_iter();assert_eq!(**it.next().as_ref().unwrap(),7);}",
        ),
        (
            "owned_iter_inspecting_drop",
            false,
            "struct Inspect<'a>(&'a u32);impl Drop for Inspect<'_>{fn drop(&mut self){assert_eq!(*self.0,7);}}fn main(){let it;let data=vec![7];let mut q=VecDeque::new();q.push_back(Inspect(&data[0]));it=q.into_iter();let _=it.len();}",
        ),
        (
            "owning_send",
            true,
            "fn need<T:Send>(_:T){}fn main(){need(VecDeque::from([std::cell::Cell::new(1)]));}",
        ),
        (
            "rc_not_send",
            false,
            "fn need<T:Send>(_:T){}fn main(){need(VecDeque::from([std::rc::Rc::new(1)]));}",
        ),
        (
            "shared_iter_not_send",
            false,
            "fn need<T:Send>(_:T){}fn main(){let q=VecDeque::from([std::cell::Cell::new(1)]);need(q.iter());}",
        ),
        (
            "mut_iter_send",
            true,
            "fn need<T:Send>(_:T){}fn main(){let mut q=VecDeque::from([std::cell::Cell::new(1)]);need(q.iter_mut());}",
        ),
        (
            "iter_clone_no_element_clone",
            true,
            "struct NoClone;fn main(){let q=VecDeque::from([NoClone]);let _=q.iter().clone();}",
        ),
        (
            "mut_iter_no_clone",
            false,
            "fn main(){let mut q=VecDeque::from([1]);let _=q.iter_mut().clone();}",
        ),
        (
            "mut_iter_no_alias",
            false,
            "fn main(){let mut q=VecDeque::from([1]);let a=q.front_mut().unwrap();let b=q.back_mut().unwrap();*a+=*b;}",
        ),
        (
            "iter_no_escape",
            false,
            "fn bad()->std::collections::vec_deque::Iter<'static,u32>{let q=VecDeque::from([1]);q.iter()}fn main(){}",
        ),
        (
            "iter_holds_borrow",
            false,
            "fn main(){let mut q=VecDeque::from([1]);let mut it=q.iter();q.push_back(2);let _=it.next();}",
        ),
    ];
    let root = std::env::temp_dir().join(format!("flux-deque-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expected, body) in cases {
        let source = format!("use std::collections::VecDeque;{body}");
        let file = root.join(format!("{name}.rs"));
        std::fs::write(&file, &source).unwrap();
        let output = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            output.status.success(),
            expected,
            "rustc {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", &source);
        assert!(hir.diagnostics.is_empty(), "{name}: {:?}", hir.diagnostics);
        let typed = flux_mir::check_file(&hir);
        let admitted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(admitted, expected, "Flux {name}: {:?}", typed.diagnostics);
    }
}

#[test]
fn host_panic_cleanup_preserves_initialized_ring() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/vec-deque-host/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    // Stable rustc cannot accept the internal dropck attribute. Remove only
    // that declaration marker; all ring/drop algorithm bodies remain unchanged.
    // The minimal RawVec adapter supplies allocation-only storage, while real
    // Flux/rustc contract cases above independently check may_dangle admission.
    let source = std::fs::read_to_string(repo.join("library/alloc/src/collections/vec_deque.rs"))
        .unwrap()
        .replace(
            "unsafe impl<#[may_dangle] T> Drop for VecDeque<T>",
            "impl<T> Drop for VecDeque<T>",
        );
    let actual = root.join("ring.rs");
    std::fs::write(&actual, source).unwrap();
    let driver = root.join("panic.rs");
    std::fs::write(
        &driver,
        include_str!("fixtures/vec_deque/panic.rs.txt")
            .replace("__SOURCE__", actual.to_str().unwrap()),
    )
    .unwrap();
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
        success(&run(&mut rustc, &root, &format!("host-build-{optimized}")));
        let output = run(
            &mut Command::new(exe),
            &root,
            &format!("host-run-{optimized}"),
        );
        success(&output);
        assert_eq!(output.stdout, b"panic invariants complete\n");
    }
}
