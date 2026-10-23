//! Slice Arc ownership bounds; rejected inputs are compile-only.
use std::process::Command;

#[test]
fn slice_ownership_bounds() {
    let dir = std::env::temp_dir().join(format!("flux-arc-slice-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in [
        (
            "move",
            "struct N; fn f(v: Vec<N>) -> Arc<[N]> { v.into() }",
            true,
        ),
        (
            "collect",
            "struct N; fn f(v: Vec<N>) -> Arc<[N]> { v.into_iter().collect() }",
            true,
        ),
        (
            "clone",
            "struct N; fn f(a: Arc<[N]>, w: Weak<[N]>) { let _ = a.clone(); let _ = w.clone(); let _ = w.upgrade(); }",
            true,
        ),
        (
            "send_sync",
            "fn need<T: Send + Sync>() {} fn f() { need::<Arc<[u32]>>(); need::<Weak<[u32]>>(); }",
            true,
        ),
        (
            "borrow",
            "fn f<'a>(x: &'a u32) -> Arc<[&'a u32]> { vec![x].into() }",
            true,
        ),
        (
            "covariant",
            "fn f<'a, 'b: 'a>(x: Arc<[&'b u32]>) -> Arc<[&'a u32]> { x } fn g<'a, 'b: 'a>(x: Weak<[&'b u32]>) -> Weak<[&'a u32]> { x }",
            true,
        ),
        (
            "cell_send",
            "fn need<T: Send>() {} fn f() { need::<Arc<[std::cell::Cell<u32>]>>(); }",
            false,
        ),
        (
            "cell_sync",
            "fn need<T: Sync>() {} fn f() { need::<Weak<[std::cell::Cell<u32>]>>(); }",
            false,
        ),
        (
            "raw_send",
            "fn need<T: Send>() {} fn f() { need::<Weak<[*mut u8]>>(); }",
            false,
        ),
        (
            "escape",
            "fn f() -> Arc<[&'static u32]> { let x = 1; vec![&x].into() }",
            false,
        ),
        (
            "weak_escape",
            "fn f() -> Weak<[&'static u32]> { let x = 1; let a: Arc<[_]> = vec![&x].into(); Arc::downgrade(&a) }",
            false,
        ),
        (
            "use_after_drop",
            "fn f(a: Arc<[u32]>) { let r = &a[0]; drop(a); println!(\"{}\", r); }",
            false,
        ),
        ("mutable", "fn f(mut a: Arc<[u32]>) { a[0] = 2; }", false),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(
            &file,
            format!("use std::sync::{{Arc, Weak}};\n{source}\nfn main() {{}}\n"),
        )
        .unwrap();
        let rustc = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join(format!("{name}.rmeta")))
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
            .arg(&file)
            .output()
            .unwrap();
        assert_eq!(
            flux.status.success(),
            accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn erased_layout_contract_and_overflow() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/arc-slice/layout-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // Inspect a test-only library module. Production code and allocator calls
    // remain unchanged; only embedded-library paths and abort spelling adapt.
    let library = include_str!("../../../library/alloc/src/sync.rs")
        .replace("use crate::alloc::", "use std::alloc::")
        .replace("use crate::vec::Vec;", "use std::vec::Vec;")
        .replace(
            "crate::raw_vec::capacity_overflow()",
            "panic!(\"capacity overflow\")",
        );
    for (name, body, succeeds) in [
        (
            "stable_layout",
            r#"
            let a = Arc::new([3u32, 4]);
            let b: Arc<[u32]> = vec![3u32, 4].into();
            unsafe {
                assert_eq!((*a.ptr.as_ptr()).layout, (*b.ptr.as_ptr()).layout);
                let a_offset = a.data.as_ptr() as usize - a.ptr.as_ptr() as usize;
                let b_offset = b.data.as_ptr() as *mut u32 as usize - b.ptr.as_ptr() as usize;
                assert_eq!(a_offset, b_offset);
                assert!(a_offset >= size_of::<Header>());
            }
            #[repr(align(512))] struct A;
            let a: Arc<[A]> = Vec::new().into();
            assert_eq!(a.data.as_ptr() as *mut A as usize % 512, 0);
            let w = Arc::downgrade(&a);
            drop(a);
            assert!(w.upgrade().is_none());
            drop(w);
        "#,
            true,
        ),
        (
            "combined_overflow",
            "let _ = allocate(Layout::from_size_align(isize::MAX as usize - 7, 8).unwrap());",
            false,
        ),
        (
            "alignment_overflow",
            "let _ = allocate(Layout::from_size_align(0, 1usize << 63).unwrap());",
            false,
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        let source = format!(
            "mod implementation {{\n{library}\npub fn exercise() {{ {body} }}\n}}\nfn main() {{ implementation::exercise(); }}\n"
        );
        // The source is ordinary valid Rust in all cases, including checked
        // layout failures. Rustc metadata acceptance precedes every execution.
        std::fs::write(
            &file,
            source.replace("core::panicking::abort_now()", "std::process::abort()"),
        )
        .unwrap();
        let metadata = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&file)
            .arg("-o")
            .arg(dir.join("test.rmeta"))
            .output()
            .unwrap();
        assert!(
            metadata.status.success(),
            "{}",
            String::from_utf8_lossy(&metadata.stderr)
        );
        for compiler in ["rustc", "flux"] {
            std::fs::write(
                &file,
                if compiler == "rustc" {
                    source.replace("core::panicking::abort_now()", "std::process::abort()")
                } else {
                    source.clone()
                },
            )
            .unwrap();
            for optimized in [false, true] {
                let executable = dir.join(format!("{name}-{compiler}-{optimized}"));
                let mut command = if compiler == "rustc" {
                    let mut c = Command::new("rustc");
                    c.args(["--edition=2024", "-C", "panic=abort"]);
                    c
                } else {
                    let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
                    c.arg("build");
                    c
                };
                if optimized {
                    command.arg("-O");
                }
                let result = command
                    .arg(&file)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    result.status.success(),
                    "{name} {compiler}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                let result = Command::new(executable)
                    .env_remove("RUST_BACKTRACE")
                    .output()
                    .unwrap();
                assert_eq!(
                    result.status.success(),
                    succeeds,
                    "{name} {compiler}: {}",
                    String::from_utf8_lossy(&result.stderr)
                );
                if !succeeds {
                    use std::os::unix::process::ExitStatusExt;
                    assert_eq!(result.status.signal(), Some(6));
                    assert!(String::from_utf8_lossy(&result.stderr).contains("capacity overflow"));
                }
            }
        }
    }
}
