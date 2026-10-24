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

// Both executables use the same source path, so panic locations must match.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn compare_executables(file: &Path, dir: &Path, name: &str, panic: Option<&str>) {
    for optimized in [false, true] {
        let mut results = Vec::new();
        for compiler in ["rustc", "flux"] {
            let executable = dir.join(format!("{name}-{compiler}-{optimized}"));
            let mut command = if compiler == "rustc" {
                let mut c = Command::new("rustc");
                c.args([
                    "--edition=2024",
                    "-C",
                    "panic=abort",
                    "-C",
                    "overflow-checks=off",
                ]);
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
                .arg(file)
                .arg("-o")
                .arg(&executable)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{name} {compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = Command::new(executable)
                .env_remove("RUST_BACKTRACE")
                .output()
                .unwrap();
            std::fs::write(
                dir.join(format!("{name}-{compiler}-{optimized}.stdout")),
                &output.stdout,
            )
            .unwrap();
            std::fs::write(
                dir.join(format!("{name}-{compiler}-{optimized}.stderr")),
                &output.stderr,
            )
            .unwrap();
            let stderr = String::from_utf8(output.stderr).unwrap();
            let stderr = stderr
                .split_inclusive('\n')
                .map(|line| {
                    if line.starts_with("thread 'main' (") {
                        let suffix = line.split_once(") panicked at ").unwrap().1;
                        format!("thread 'main' (N) panicked at {suffix}")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<String>();
            if let Some(message) = panic {
                use std::os::unix::process::ExitStatusExt;
                assert_eq!(output.status.signal(), Some(6), "{name} {compiler}");
                assert!(stderr.contains(message), "{stderr}");
            } else {
                assert!(output.status.success(), "{name} {compiler}: {stderr}");
                assert!(stderr.is_empty(), "{stderr}");
            }
            results.push((output.status.code(), output.stdout, stderr));
        }
        assert_eq!(results[0], results[1], "{name} optimized={optimized}");
    }
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
fn unchanged_shared_slice_source_matches_rustc() {
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
        assert!(flux.status.success(), "{errors}");
        assert!(errors.is_empty(), "{errors}");
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            compare_executables(&file, &dir, name, None);
        }
    }
}

#[test]
fn iterator_projection_boundaries_are_compile_only() {
    let dir = std::env::temp_dir().join(format!("flux-iterator-projection-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source, accepts) in [
        (
            "borrowed",
            "fn f<'a, I: IntoIterator<Item=&'a u32>>(x:I) where I::IntoIter: ExactSizeIterator { let it=x.into_iter(); if it.len()==0 { assert_eq!(it.count(),0); return; } for v in it { let _: &u32=v; } }",
            true,
        ),
        (
            "borrowed_escape",
            "fn f<'a, I: IntoIterator<Item=&'a u32>>(x:I) -> Option<&'static u32> where I::IntoIter: ExactSizeIterator { let mut it=x.into_iter(); if it.len()==0 { assert_eq!(it.count(),0); return None; } it.next() }",
            false,
        ),
        (
            "local_escape",
            "fn f() -> &'static u32 { let xs=[1u32]; let mut it=xs.iter(); it.next().unwrap() }",
            false,
        ),
        (
            "missing_exact",
            "fn f<I: IntoIterator<Item=u32>>(x:I) { let it=x.into_iter(); let _=it.len(); }",
            false,
        ),
        (
            "missing_iterator",
            "fn f<I>(x:I) { let _=x.count(); }",
            false,
        ),
        (
            "conflicting_item",
            "fn f<I: IntoIterator<Item=u32>>(x:I) where I::IntoIter: ExactSizeIterator + Iterator<Item=u64> { let it=x.into_iter(); for v in it { let _:u32=v; } }",
            false,
        ),
        (
            "wrong_item",
            "fn f<I: IntoIterator<Item=u32>>(x:I) where I::IntoIter: ExactSizeIterator { for v in x.into_iter() { let _:u64=v; } }",
            false,
        ),
        (
            "count_moves",
            "fn f<I: IntoIterator<Item=u32>>(x:I) where I::IntoIter: ExactSizeIterator { let it=x.into_iter(); let _=it.count(); let _=it.count(); }",
            false,
        ),
        (
            "loop_moves",
            "fn f<I: IntoIterator<Item=u32>>(x:I) where I::IntoIter: ExactSizeIterator { let it=x.into_iter(); for _ in it {} let _=it.count(); }",
            false,
        ),
        (
            "distinct_trait_arguments",
            "trait Get<T> { fn get(&self)->T; } trait Both: Get<u8> {} trait Holder { type Part: Get<u16>; fn part(self)->Self::Part; } fn f<H:Holder>(h:H)->(u8,u16) where H::Part:Both { let p=h.part(); (Get::<u8>::get(&p),Get::<u16>::get(&p)) }",
            true,
        ),
        (
            "iterator_blanket_conversion",
            "fn f<I>(x:I) where I: ExactSizeIterator<Item=u32> { let _=x.into_iter().count(); }",
            true,
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, format!("{source}\nfn main() {{}}\n")).unwrap();
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
        let errors = String::from_utf8_lossy(&flux.stderr);
        let expected: &[&str] = match name {
            "borrowed_escape" => &["cannot return value referencing local variable `it`"],
            "local_escape" => &["cannot return value referencing local variable `xs`"],
            "missing_exact" => &[
                "no method named `len` found for type `<I as IntoIterator>::IntoIter` in the current scope",
            ],
            "missing_iterator" => &[
                "the method `count` exists for type parameter `I`, but its trait bounds were not satisfied",
            ],
            "conflicting_item" => &[
                "expected `<I as IntoIterator>::IntoIter` to be an iterator that yields `u32`, but it yields `u64`",
                "mismatched types: expected `u32`, found `u64`",
            ],
            "wrong_item" => &["mismatched types: expected `u64`, found `u32`"],
            "count_moves" | "loop_moves" => &["use of moved value: `it`"],
            _ => &[],
        };
        let actual: Vec<_> = errors
            .lines()
            .map(|line| line.split_once(": error: ").unwrap().1)
            .collect();
        assert_eq!(actual, expected, "{name}: {errors}");
    }
    // Preserve the independent borrowed-return region gap with exact diagnostics.
    let file = dir.join("borrowed_iterator_return.rs");
    std::fs::write(
        &file,
        include_str!("fixtures/sized_arc/borrowed_iterator_return.rs.txt"),
    )
    .unwrap();
    assert!(
        rustc_metadata(&file, &dir.join("test.rmeta"))
            .status
            .success()
    );
    let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("mir")
        .arg(&file)
        .output()
        .unwrap();
    assert!(!flux.status.success());
    assert_eq!(
        String::from_utf8_lossy(&flux.stderr),
        format!(
            "{}:12:5: error: cannot return value referencing local variable `it`\n",
            file.display()
        )
    );
    // Trait lifetime declarations are still outside the public subset.
    let file = dir.join("unsupported_trait_lifetime.rs");
    std::fs::write(&file, "trait Tr<'a> { type Item; } fn main() {}\n").unwrap();
    assert!(
        rustc_metadata(&file, &dir.join("test.rmeta"))
            .status
            .success()
    );
    let flux = Command::new(env!("CARGO_BIN_EXE_flux"))
        .arg("mir")
        .arg(file)
        .output()
        .unwrap();
    assert!(!flux.status.success());
    assert!(
        String::from_utf8_lossy(&flux.stderr)
            .contains("lifetime parameters on traits are not supported yet")
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unchanged_shared_slice_rejects_malicious_iterator_lengths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join(format!(
        "out/shared-slice-inference/panics-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let original = root.join("crates/flux-arena/src/shared_slice.rs");
    let copied = dir.join("shared_slice.rs");
    std::fs::copy(&original, &copied).unwrap();
    assert_eq!(
        std::fs::read(original).unwrap(),
        std::fs::read(copied).unwrap()
    );
    for (name, actual, claimed) in [
        ("too_long", 2, 1),
        ("too_short", 1, 2),
        ("false_empty", 1, 0),
    ] {
        let file = dir.join(format!("{name}.rs"));
        let source = include_str!("fixtures/sized_arc/malicious_iterator.rs.txt")
            .replace("ACTUAL", &actual.to_string())
            .replace("CLAIMED", &claimed.to_string());
        std::fs::write(&file, source).unwrap();
        assert!(
            rustc_metadata(&file, &dir.join("test.rmeta"))
                .status
                .success()
        );
        compare_executables(&file, &dir, name, Some("incorrect iterator length"));
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
    // An ordinary test module must not claim the trusted alloc lang item.
    let library = include_str!("../../../library/alloc/src/sync.rs")
        .replace("#[lang = \"arc\"]\n", "")
        .replace("use crate::alloc::", "use std::alloc::")
        .replace("use crate::vec::Vec;", "use std::vec::Vec;")
        .replace(
            "crate::raw_vec::capacity_overflow()",
            "panic!(\"capacity overflow\")",
        );
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
