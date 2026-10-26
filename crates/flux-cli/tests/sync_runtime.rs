//! Runtime and ownership oracles for the bounded synchronization APIs.
//! Invalid ownership and marker cases are checked without execution.
mod support;

use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(dir: &Path, name: &str, command: &mut Command) -> Output {
    support::process::run(command, dir, name, Duration::from_secs(60)).completed()
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn compare_runtime(name: &str, expected: &[u8]) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join(format!("out/sync-runtime/{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let file = root.join(format!("tests/run/{name}.rs"));
    for optimized in [false, true] {
        let mut results = Vec::new();
        for compiler in ["rustc", "flux"] {
            let exe = dir.join(format!("{compiler}-{optimized}"));
            let mut cmd = if compiler == "rustc" {
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
            cmd.arg(&file).arg("-o").arg(&exe);
            if optimized {
                cmd.arg("-O");
            }
            let output = bounded(&dir, &format!("compile-{compiler}-{optimized}"), &mut cmd);
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let output = bounded(
                &dir,
                &format!("run-{compiler}-{optimized}"),
                &mut Command::new(&exe),
            );
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert!(output.stderr.is_empty());
            assert_eq!(output.stdout, expected);
            results.push(output.stdout);
        }
        assert_eq!(results[0], results[1]);
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn sync_runtime_matches_rustc() {
    compare_runtime("sync_runtime", b"sync 1 3 8 17 2 32\n");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn mutex_debug_does_not_wait_for_held_lock() {
    compare_runtime("mutex_debug", b"Mutex { data: [7, 11], poisoned: false, .. }\nMutex { data: \"<locked>\", poisoned: false, .. }\nMutex { data: [7, 11, 13], poisoned: false, .. }\nMutex {\n    data: 42,\n    poisoned: false,\n    ..\n}\n");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn constant_function_pointers_match_rustc() {
    compare_runtime("const_function_pointers", b"functions 17 -3 42 23 29\n");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn unused_function_pointer_does_not_collect_recursive_target() {
    compare_runtime(
        "const_unused_function_pointer",
        b"unused constant remains unused\n",
    );
}

#[test]
fn actual_implementations_host_contention_and_poison() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join("out/sync-runtime/host");
    std::fs::create_dir_all(&dir).unwrap();
    let mut source = String::from("pub use std::thread;\nmod implementation {\n");
    for module in ["once_lock", "lazy_lock", "mutex", "rwlock", "poison"] {
        source.push_str(&format!(
            "#[path = {:?}] mod {module};\n",
            root.join(format!("library/std/src/sync/{module}.rs"))
        ));
    }
    source.push_str("pub use self::once_lock::OnceLock;\npub use self::lazy_lock::LazyLock;\npub use self::mutex::*;\npub use self::rwlock::*;\npub use self::poison::*;\n}\n");
    source.push_str(include_str!("fixtures/sync_runtime/host.rs.txt"));
    let file = dir.join("host.rs");
    std::fs::write(&file, source).unwrap();
    for optimized in [false, true] {
        let exe = dir.join(format!("host-{optimized}"));
        let mut cmd = Command::new("rustc");
        cmd.args(["--edition=2024", "-o"]).arg(&exe).arg(&file);
        if optimized {
            cmd.arg("-O");
        }
        let output = bounded(&dir, "compile", &mut cmd);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let output = bounded(&dir, "run", &mut Command::new(exe));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn ownership_and_marker_contracts_match_rustc() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("out/sync-runtime/contracts");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body, accepts) in [
        (
            "owners",
            "fn f() { send::<Box<Cell<u32>>>(); sync::<Box<u32>>(); send::<Vec<Cell<u32>>>(); sync::<Vec<u32>>(); }",
            true,
        ),
        (
            "box_not_sync",
            "fn f() { sync::<Box<Cell<u32>>>(); }",
            false,
        ),
        (
            "vec_not_sync",
            "fn f() { sync::<Vec<Cell<u32>>>(); }",
            false,
        ),
        ("box_not_send", "fn f() { send::<Box<*mut u8>>(); }", false),
        ("vec_not_send", "fn f() { send::<Vec<*mut u8>>(); }", false),
        (
            "mutex_cell",
            "fn f() { send::<Mutex<Cell<u32>>>(); sync::<Mutex<Cell<u32>>>(); }",
            true,
        ),
        (
            "mutex_not_send",
            "fn f() { sync::<Mutex<*mut u8>>(); }",
            false,
        ),
        (
            "rw_markers",
            "fn f() { send::<RwLock<Cell<u32>>>(); sync::<RwLock<u32>>(); }",
            true,
        ),
        (
            "rw_not_sync",
            "fn f() { sync::<RwLock<Cell<u32>>>(); }",
            false,
        ),
        (
            "mutex_guard_not_send",
            "fn f<'a>() { send::<MutexGuard<'a, u32>>(); }",
            false,
        ),
        (
            "reader_not_send",
            "fn f<'a>() { send::<RwLockReadGuard<'a, u32>>(); }",
            false,
        ),
        (
            "writer_not_send",
            "fn f<'a>() { send::<RwLockWriteGuard<'a, u32>>(); }",
            false,
        ),
        (
            "guards_sync",
            "fn f<'a>() { sync::<MutexGuard<'a, u32>>(); sync::<RwLockReadGuard<'a, u32>>(); sync::<RwLockWriteGuard<'a, u32>>(); }",
            true,
        ),
        (
            "guard_not_sync",
            "fn f<'a>() { sync::<MutexGuard<'a, Cell<u32>>>(); }",
            false,
        ),
        (
            "lazy_markers",
            "fn f() { send::<LazyLock<u32>>(); sync::<LazyLock<u32>>(); }",
            true,
        ),
        (
            "lazy_not_sync",
            "fn f() { sync::<LazyLock<Cell<u32>>>(); }",
            false,
        ),
        (
            "guard_escape",
            "fn f() -> &'static u32 { let lock = Mutex::new(3); let guard = lock.lock().unwrap(); &*guard }",
            false,
        ),
        (
            "not_copy",
            "fn f() { let a = Mutex::new(3); let b = a; drop(a); drop(b); }",
            false,
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(&file, format!("use std::sync::{{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard, LazyLock}}; use std::cell::Cell; fn send<T: Send>() {{}} fn sync<T: Sync>() {{}}\n{body}\nfn main() {{}}\n")).unwrap();
        for compiler in ["rustc", "flux"] {
            let mut cmd = if compiler == "rustc" {
                let mut c = Command::new("rustc");
                c.args(["--edition=2024", "--emit=metadata", "-o"])
                    .arg(dir.join(format!("{name}.rmeta")));
                c
            } else {
                let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
                c.arg("mir");
                c
            };
            let output = bounded(&dir, &format!("{name}-{compiler}"), cmd.arg(&file));
            assert_eq!(
                output.status.success(),
                accepts,
                "{name} {compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
