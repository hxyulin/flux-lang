//! Main-thread and worker TLS exit lifecycle, compared with rustc.
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    let stdout = directory.join(format!("{label}.stdout"));
    let stderr = directory.join(format!("{label}.stderr"));
    let mut child = command.stdout(Stdio::from(std::fs::File::create(&stdout).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&stderr).unwrap())).spawn().unwrap();
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if started.elapsed() > Duration::from_secs(30) {
            child.kill().unwrap(); child.wait().unwrap(); panic!("{label} exceeded 30 seconds");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    Output { status, stdout: std::fs::read(stdout).unwrap(), stderr: std::fs::read(stderr).unwrap() }
}


#[test]
#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn main_and_worker_destructors_match_rustc() {
    compare_exit("tls_exit_lifecycle", b"workers 3 3 3\nmain tls dropped 4 4 4\n");
}

#[test]
#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn partial_line_from_main_tls_reaches_stdout() {
    compare_exit("tls_exit_tail", b"bodytail");
}

#[test]
#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn dropless_keys_outlive_payload_destructors() {
    compare_exit("tls_exit_dropless", b"value 19\n");
}

#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn compare_exit(name: &str, stdout: &[u8]) {
    let root=std::env::temp_dir().join(format!("flux-{name}-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source=Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../tests/run/{name}.rs"));
    let reference=root.join("rustc");
    let compiled=bounded(Command::new("rustc").args(["--edition=2024","-Cpanic=abort","-o"]).arg(&reference).arg(&source),&root,"compile-rustc");
    assert!(compiled.status.success(),"{}",String::from_utf8_lossy(&compiled.stderr));
    let expected=bounded(&mut Command::new(&reference),&root,"run-rustc");
    assert!(expected.status.success(),"{}",String::from_utf8_lossy(&expected.stderr));
    assert_eq!(expected.stdout,stdout);
    for optimized in [false,true] {
        let executable=root.join(format!("flux-{optimized}"));
        let mut command=Command::new(env!("CARGO_BIN_EXE_flux"));command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {command.arg("-O");}
        let compiled=bounded(&mut command,&root,&format!("compile-flux-{optimized}"));
        assert!(compiled.status.success(),"{}",String::from_utf8_lossy(&compiled.stderr));
        for repeat in 0..5 {
            let reference_run=bounded(&mut Command::new(&reference),&root,&format!("run-rustc-{optimized}-{repeat}"));
            assert_eq!(reference_run.status.code(),expected.status.code());
            assert_eq!(reference_run.stdout,expected.stdout);assert_eq!(reference_run.stderr,expected.stderr);
            let actual=bounded(&mut Command::new(&executable),&root,&format!("run-flux-{optimized}-{repeat}"));
            assert_eq!(actual.status.code(),expected.status.code(),"{}",String::from_utf8_lossy(&actual.stderr));
            assert_eq!(actual.stdout,expected.stdout);assert_eq!(actual.stderr,expected.stderr);
        }
    }
}

#[test]
#[cfg(all(target_os="macos",target_arch="aarch64"))]
fn recursive_initialization_still_aborts() {
    let root=std::env::temp_dir().join(format!("flux-tls-recursion-{}",std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let source=root.join("main.rs");
    std::fs::write(&source,"thread_local!{static VALUE:u32=VALUE.with(|value|*value);}fn main(){VALUE.with(|_|());}").unwrap();
    let reference=root.join("rustc");
    let compiled=bounded(Command::new("rustc").args(["--edition=2024","-Cpanic=abort","-o"]).arg(&reference).arg(&source),&root,"compile-rustc");
    assert!(compiled.status.success(),"{}",String::from_utf8_lossy(&compiled.stderr));
    let expected=bounded(&mut Command::new(reference),&root,"run-rustc");
    assert!(!expected.status.success());assert!(expected.stdout.is_empty());
    for optimized in [false,true] {
        let executable=root.join(format!("flux-{optimized}"));
        let mut command=Command::new(env!("CARGO_BIN_EXE_flux"));command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {command.arg("-O");}
        let compiled=bounded(&mut command,&root,&format!("compile-flux-{optimized}"));
        assert!(compiled.status.success(),"{}",String::from_utf8_lossy(&compiled.stderr));
        let actual=bounded(&mut Command::new(executable),&root,&format!("run-flux-{optimized}"));
        assert!(!actual.status.success());assert!(actual.stdout.is_empty());
        assert!(String::from_utf8_lossy(&actual.stderr).contains("recursively initialize thread-local storage"));
    }
}
