//! Compile-only ownership/marker/lifetime contracts. Invalid Rust is never run.
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(dir: &Path, name: &str, command: &mut Command) -> Output {
    let out = dir.join(format!("{name}.stdout"));
    let err = dir.join(format!("{name}.stderr"));
    let mut child = command
        .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap()))
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{name} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output {
        status,
        stdout: std::fs::read(out).unwrap(),
        stderr: std::fs::read(err).unwrap(),
    }
}

#[test]
fn compile_only_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let dir = root.join("out/once-lock/contracts");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, body, accepts) in [
        (
            "markers",
            "fn send<T: Send>() {} fn sync<T: Sync>() {} fn f() { send::<OnceLock<Cell<u32>>>(); send::<OnceLock<usize>>(); sync::<OnceLock<usize>>(); }",
            true,
        ),
        (
            "not_sync",
            "fn sync<T: Sync>() {} fn f() { sync::<OnceLock<Cell<u32>>>(); }",
            false,
        ),
        (
            "not_send",
            "fn send<T: Send>() {} fn f() { send::<OnceLock<*mut u8>>(); }",
            false,
        ),
        (
            "sync_requires_send",
            "struct N(*mut u8); unsafe impl Sync for N {} fn sync<T: Sync>() {} fn f() { sync::<OnceLock<N>>(); }",
            false,
        ),
        (
            "borrowed",
            "fn f<'a>(x: &'a u32) { let c = OnceLock::new(); assert_eq!(**c.get_or_init(|| x), *x); }",
            true,
        ),
        (
            "invariant",
            "fn f<'a, 'b: 'a>(x: OnceLock<&'b u32>) -> OnceLock<&'a u32> { x }",
            false,
        ),
        (
            "equal",
            "fn f<'a: 'b, 'b: 'a>(x: OnceLock<&'b u32>) -> OnceLock<&'a u32> { x }",
            true,
        ),
        (
            "escape",
            "fn f() -> &'static u32 { let c = OnceLock::new(); c.get_or_init(|| 5) }",
            false,
        ),
        (
            "payload_escape",
            "fn f() -> OnceLock<&'static u32> { let x = 5; let c = OnceLock::new(); c.get_or_init(|| &x); c }",
            false,
        ),
        (
            "drop_borrowed",
            "fn f() { let c = OnceLock::new(); let r = c.get_or_init(|| 5); drop(c); println!(\"{}\", r); }",
            false,
        ),
        (
            "not_copy",
            "fn f() { let c: OnceLock<u32> = OnceLock::new(); let d = c; drop(c); drop(d); }",
            false,
        ),
        ("sized", "fn f(x: &OnceLock<[u32]>) {}", false),
        (
            "fn_once",
            "fn f() { let c = OnceLock::new(); let s = String::from(\"x\"); c.get_or_init(|| s); }",
            true,
        ),
    ] {
        let file = dir.join(format!("{name}.rs"));
        std::fs::write(
            &file,
            format!("use std::sync::OnceLock; use std::cell::Cell;\n{body}\nfn main() {{}}\n"),
        )
        .unwrap();
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

#[test]
fn actual_implementation_host_unwind_and_contention() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join("out/once-lock/host");
    std::fs::create_dir_all(&dir).unwrap();
    let source = format!(
        "#[path = {:?}] mod implementation;\n{}",
        root.join("library/std/src/sync/once_lock.rs"),
        include_str!("fixtures/once_lock/host.rs.txt")
    );
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
        let output = bounded(&dir, &format!("run-{optimized}"), &mut Command::new(exe));
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn sequential_oracle_in_both_modes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let dir = root.join("out/once-lock/sequential-oracles");
    std::fs::create_dir_all(&dir).unwrap();
    let file = root.join("tests/run/once_lock.rs");
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
            assert_eq!(output.stdout, b"once-lock 41 owned 2 73 11\n");
            results.push(output.stdout);
        }
        assert_eq!(results[0], results[1]);
    }
}
