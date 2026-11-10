//! Scoped native workers and their lifetime contracts use rustc as an oracle.
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    support::process::run(command, directory, label, Duration::from_secs(30)).completed()
}

#[test]
fn scope_lifetime_and_send_contracts_match_rustc() {
    let directory =
        std::env::temp_dir().join(format!("flux-scope-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for (name, source, accepts) in [
        (
            "borrow",
            "fn main(){let mut n=1;std::thread::scope(|s|{s.spawn(||{n+=4;});});assert_eq!(n,5);}",
            true,
        ),
        (
            "borrowed_result",
            "fn main(){let value=String::from(\"data\");std::thread::scope(|s|{assert_eq!(s.spawn(||value.as_str()).join().unwrap(),\"data\");});}",
            true,
        ),
        (
            "handle_escape",
            "fn main(){let h=std::thread::scope(|s|s.spawn(||3));let _=h.join();}",
            false,
        ),
        (
            "scope_escape",
            "fn main(){let _=std::thread::scope(|s|s);}",
            false,
        ),
        (
            "local_capture",
            "fn main(){std::thread::scope(|s|{let local=String::from(\"short\");s.spawn(||local.len());});}",
            false,
        ),
        (
            "local_result",
            "fn main(){let _=std::thread::scope(|s|{let local=String::from(\"short\");s.spawn(||local.as_str()).join().unwrap()});}",
            false,
        ),
        (
            "non_send_closure",
            "fn main(){let rc=std::rc::Rc::new(3);std::thread::scope(|s|{s.spawn(move||drop(rc));});}",
            false,
        ),
        (
            "non_send_result",
            "fn main(){std::thread::scope(|s|{s.spawn(||std::rc::Rc::new(3));});}",
            false,
        ),
        (
            "non_sync_borrow",
            "fn main(){let cell=std::cell::Cell::new(0);std::thread::scope(|s|{s.spawn(||cell.set(1));});}",
            false,
        ),
        (
            "invariant_scope",
            "fn main(){std::thread::scope(|s|{s.spawn(||{let n=3;s.spawn(||println!(\"{}\",n));});});}",
            false,
        ),
    ] {
        if !support::cases::selected(name) {
            continue;
        }
        let path = directory.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let reference = bounded(
            Command::new("rustc")
                .args(["--edition=2024", "--emit=metadata", "-o"])
                .arg(directory.join(format!("{name}.rmeta")))
                .arg(&path),
            &directory,
            &format!("rustc-{name}"),
        );
        assert_eq!(
            reference.status.success(),
            accepts,
            "rustc {name}: {}",
            String::from_utf8_lossy(&reference.stderr)
        );
        let actual = bounded(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("mir")
                .arg(&path),
            &directory,
            &format!("flux-{name}"),
        );
        assert_eq!(
            actual.status.success(),
            accepts,
            "flux {name}: {}",
            String::from_utf8_lossy(&actual.stderr)
        );
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn real_scoped_workers_match_rustc() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let directory = std::env::temp_dir().join(format!("flux-scoped-native-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let source = repository.join("tests/run/scoped_threads.rs");
    let reference = directory.join("rustc");
    let output = bounded(
        Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &directory,
        "compile-rustc",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = bounded(&mut Command::new(&reference), &directory, "run-rustc");
    assert!(expected.status.success());
    assert_eq!(expected.stdout, b"scoped 4 5 6 17\n");
    for optimized in [false, true] {
        let executable = directory.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let output = bounded(
            &mut command,
            &directory,
            &format!("compile-flux-{optimized}"),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual = bounded(
            &mut Command::new(&executable),
            &directory,
            &format!("run-flux-{optimized}"),
        );
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

#[test]
fn scope_cleanup_joins_on_host_unwind() {
    let directory = std::env::temp_dir().join(format!("flux-scoped-unwind-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let module = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../library/std/src/thread/scoped.rs")
        .canonicalize()
        .unwrap();
    let source = format!(
        r#"
extern crate alloc as alloc_crate;
type Result<T> = std::thread::Result<T>;
pub use std::panic;
pub use std::thread::panicking;
mod sync {{ pub use std::sync::Mutex; }}
use std::io;
// The worker's thread handle; this harness checks joining only.
#[derive(Clone)]
pub struct Thread;
impl Thread {{ pub(crate) fn new(_name: Option<String>) -> Thread {{ Thread }} }}
pub(crate) fn set_current(_thread: Thread) {{}}
#[path={module:?}]mod scoped;
use std::sync::atomic::{{AtomicBool,Ordering}};
struct Release<'a>(&'a AtomicBool);
impl Drop for Release<'_>{{fn drop(&mut self){{self.0.store(true,Ordering::SeqCst);}}}}
fn main(){{
    let ready=AtomicBool::new(false);let done=AtomicBool::new(false);
    let result=std::panic::catch_unwind(||scoped::scope(|s|{{
        s.spawn(||{{while !ready.load(Ordering::SeqCst){{std::hint::spin_loop();}}done.store(true,Ordering::SeqCst);}});
        let _release=Release(&ready);
        panic!("unwind through scope");
    }}));
    assert!(result.is_err());assert!(done.load(Ordering::SeqCst));
}}
"#
    );
    let path = directory.join("main.rs");
    std::fs::write(&path, source).unwrap();
    let executable = directory.join("host");
    let output = bounded(
        Command::new("rustc")
            .args(["--edition=2024", "-o"])
            .arg(&executable)
            .arg(&path),
        &directory,
        "compile-host",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = bounded(&mut Command::new(executable), &directory, "run-host");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
