//! Sized shared ownership boundaries and the unchanged source consumer gate.
use std::path::Path;
use std::process::Command;

fn rustc_metadata(file: &Path, output: &Path) -> std::process::Output {
    Command::new("rustc")
        .args(["--edition=2024", "--emit=metadata"])
        .arg(file)
        .arg("-o")
        .arg(output)
        .output()
        .unwrap()
}

#[test]
fn ownership_bounds_and_borrows_are_compile_only() {
    let dir = std::env::temp_dir().join(format!("flux-sized-arc-bounds-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in [
        (
            "positive",
            "fn need<T: Send + Sync>() {} fn f() { need::<Arc<usize>>(); need::<Weak<usize>>(); }",
            true,
        ),
        (
            "no_clone_bound",
            "struct N; fn f(x: &Arc<N>, w: &Weak<N>) { let _ = x.clone(); let _ = w.clone(); }",
            true,
        ),
        (
            "arc_cell_send",
            "fn need<T: Send>() {} fn f() { need::<Arc<std::cell::Cell<u32>>>(); }",
            false,
        ),
        (
            "arc_cell_sync",
            "fn need<T: Sync>() {} fn f() { need::<Arc<std::cell::Cell<u32>>>(); }",
            false,
        ),
        (
            "weak_cell_send",
            "fn need<T: Send>() {} fn f() { need::<Weak<std::cell::Cell<u32>>>(); }",
            false,
        ),
        (
            "weak_cell_sync",
            "fn need<T: Sync>() {} fn f() { need::<Weak<std::cell::Cell<u32>>>(); }",
            false,
        ),
        (
            "arc_raw_send",
            "fn need<T: Send>() {} fn f() { need::<Arc<*mut u8>>(); }",
            false,
        ),
        (
            "weak_raw_sync",
            "fn need<T: Sync>() {} fn f() { need::<Weak<*mut u8>>(); }",
            false,
        ),
        (
            "arc_sync_only_send",
            "struct N(*mut u8); unsafe impl Sync for N {} fn need<T: Send>() {} fn f() { need::<Arc<N>>(); }",
            false,
        ),
        (
            "arc_sync_only_sync",
            "struct N(*mut u8); unsafe impl Sync for N {} fn need<T: Sync>() {} fn f() { need::<Arc<N>>(); }",
            false,
        ),
        (
            "weak_sync_only_send",
            "struct N(*mut u8); unsafe impl Sync for N {} fn need<T: Send>() {} fn f() { need::<Weak<N>>(); }",
            false,
        ),
        (
            "weak_sync_only_sync",
            "struct N(*mut u8); unsafe impl Sync for N {} fn need<T: Sync>() {} fn f() { need::<Weak<N>>(); }",
            false,
        ),
        (
            "borrowed",
            "fn f<'a>(x: &'a u32) -> Arc<&'a u32> { Arc::new(x) }",
            true,
        ),
        (
            "arc_covariant",
            "fn f<'a, 'b: 'a>(x: Arc<&'b u32>) -> Arc<&'a u32> { x }",
            true,
        ),
        (
            "weak_covariant",
            "fn f<'a, 'b: 'a>(x: Weak<&'b u32>) -> Weak<&'a u32> { x }",
            true,
        ),
        (
            "drop_obligation",
            "fn f() { let a; { let x = 1u32; a = Arc::new(&x); } drop(a); }",
            false,
        ),
        (
            "arc_escape",
            "fn f() -> Arc<&'static u32> { let x = 1; Arc::new(&x) }",
            false,
        ),
        (
            "weak_escape",
            "fn f() -> Weak<&'static u32> { let x = 1; Arc::downgrade(&Arc::new(&x)) }",
            false,
        ),
        (
            "deref_escape",
            "fn f() -> &'static u32 { let x = Arc::new(1); &*x }",
            false,
        ),
        ("deref_mut", "fn f(mut x: Arc<u32>) { *x = 2; }", false),
        (
            "move_borrowed",
            "fn f(x: Arc<u32>) { let r = &*x; drop(x); println!(\"{}\", r); }",
            false,
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(
            &file,
            format!("use std::sync::{{Arc, Weak}};\n{source}\nfn main() {{}}\n"),
        )
        .unwrap();
        let rustc = rustc_metadata(&file, &dir.join("test.rmeta"));
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
            "Flux {name}: {}",
            String::from_utf8_lossy(&flux.stderr)
        );
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn unchanged_shared_slice_source_records_iterator_projection_blocker() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join(format!("out/sized-arc/source-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let original = root.join("crates/flux-arena/src/shared_slice.rs");
    let copied = dir.join("shared_slice.rs");
    std::fs::copy(&original, &copied).unwrap();
    assert_eq!(
        std::fs::read(&original).unwrap(),
        std::fs::read(&copied).unwrap()
    );
    // These are standard-library controls, not Flux acceptance: the complete
    // unchanged source's five tests include weak-current release, chunk reuse,
    // pool drop, malicious iterator lengths, ZSTs and the threaded reader case.
    let output = Command::new("rustc")
        .args(["--edition=2024", "--test"])
        .arg(&copied)
        .arg("-o")
        .arg(dir.join("rust-controls"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = Command::new(dir.join("rust-controls"))
        .arg("--test-threads=1")
        .output()
        .unwrap();
    std::fs::write(dir.join("rust-controls.stdout"), &output.stdout).unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let consumer = include_str!("fixtures/sized_arc/consumer.rs.txt").replace(
        "../../crates/flux-arena/src/shared_slice.rs",
        "shared_slice.rs",
    );
    for (name, source) in [
        ("consumer", consumer),
        (
            "iterator_projection",
            include_str!("fixtures/sized_arc/iterator_projection.rs.txt").to_owned(),
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
        if name == "consumer" {
            let output = Command::new("rustc")
                .args(["--edition=2024", "--test"])
                .arg(&file)
                .arg("-o")
                .arg(dir.join("consumer-controls"))
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(dir.join("consumer-controls"))
                .arg("--test-threads=1")
                .output()
                .unwrap();
            std::fs::write(dir.join("consumer-controls.stdout"), &output.stdout).unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        let rustc = rustc_metadata(&file, &dir.join(format!("{name}.rmeta")));
        assert!(
            rustc.status.success(),
            "{}",
            String::from_utf8_lossy(&rustc.stderr)
        );
        let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
            .arg("mir")
            .arg(&file)
            .output()
            .unwrap();
        std::fs::write(dir.join(format!("{name}.stderr")), &flux.stderr).unwrap();
        let errors = String::from_utf8_lossy(&flux.stderr);
        assert!(!flux.status.success());
        assert_eq!(errors.lines().count(), 2, "{errors}");
        assert!(
            errors
                .lines()
                .all(|line| line.ends_with("error: type annotations needed")),
            "{errors}"
        );
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn checked_counter_limits_abort_without_wrapping() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join(format!("out/sized-arc/limits-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A test-only module copy can inspect private counters without adding a
    // production debug API. Counts are exaggerated while exclusively owned;
    // this can only leak ownership, not permit premature destruction.
    let library = include_str!("../../../library/alloc/src/sync.rs")
        .replace("use crate::alloc::", "use std::alloc::");
    for (name, setup, operation) in [
        (
            "strong_clone",
            "unsafe { (*a.ptr.as_ptr()).strong.store(limit - 1, Relaxed); } let _b = a.clone(); assert_eq!(unsafe { (*a.ptr.as_ptr()).strong.load(Relaxed) }, limit);",
            "let _ = a.clone();",
        ),
        (
            "downgrade",
            "unsafe { (*a.ptr.as_ptr()).weak.store(limit - 1, Relaxed); } let _w = Arc::downgrade(&a); assert_eq!(unsafe { (*a.ptr.as_ptr()).weak.load(Relaxed) }, limit);",
            "let _ = Arc::downgrade(&a);",
        ),
        (
            "weak_clone",
            "let w = Arc::downgrade(&a); unsafe { (*a.ptr.as_ptr()).weak.store(limit - 1, Relaxed); } let _w = w.clone(); assert_eq!(unsafe { (*a.ptr.as_ptr()).weak.load(Relaxed) }, limit);",
            "let _ = w.clone();",
        ),
        (
            "upgrade",
            "let w = Arc::downgrade(&a); unsafe { (*a.ptr.as_ptr()).strong.store(limit - 1, Relaxed); } let _b = w.upgrade().unwrap(); assert_eq!(unsafe { (*a.ptr.as_ptr()).strong.load(Relaxed) }, limit);",
            "let _ = w.upgrade();",
        ),
    ] {
        let tail = format!(
            "\npub fn exercise() {{ let a = Arc::new(17usize); let limit = isize::MAX as usize; {setup} {operation} }}\n"
        );
        for (compiler, body) in [
            (
                "rustc",
                library.replace("core::panicking::abort_now()", "std::process::abort()"),
            ),
            ("flux", library.clone()),
        ] {
            let source = dir.join(format!("{name}-{compiler}.rs"));
            std::fs::write(&source, format!("mod implementation {{\n{body}{tail}\n}}\nfn main() {{ implementation::exercise(); }}\n")).unwrap();
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
                let output = command
                    .arg(&source)
                    .arg("-o")
                    .arg(&executable)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{name} {compiler}: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                let output = Command::new(&executable).output().unwrap();
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(output.status.signal(), Some(6), "{name} {compiler}");
                // A failed assertion would also abort, but prints a diagnostic.
                assert!(
                    output.stderr.is_empty(),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
            }
        }
    }
}
