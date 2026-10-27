//! Ownership contracts and host-unwind behavior of the backend prerequisites.
mod support;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[test]
fn once_cell_contracts() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/backend-std/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, body, expected) in [
        (
            "send",
            "fn send<T:Send>(){} fn f(){send::<OnceCell<String>>();}",
            true,
        ),
        (
            "not_sync",
            "fn sync<T:Sync>(){} fn f(){sync::<OnceCell<u32>>();}",
            false,
        ),
        (
            "not_send",
            "fn send<T:Send>(){} fn f(){send::<OnceCell<*mut u8>>();}",
            false,
        ),
        (
            "invariant",
            "fn f<'a,'b:'a>(x:OnceCell<&'b u32>)->OnceCell<&'a u32>{x}",
            false,
        ),
        (
            "escape",
            "fn f()->&'static u32{let c=OnceCell::new(); c.get_or_init(||7)}",
            false,
        ),
        (
            "borrowed",
            "fn f<'a>(x:&'a u32)->OnceCell<&'a u32>{let c=OnceCell::new();c.set(x).unwrap();c}",
            true,
        ),
    ] {
        let source = root.join(format!("{name}.rs"));
        std::fs::write(
            &source,
            format!("use std::cell::OnceCell;{body}fn main(){{}}"),
        )
        .unwrap();
        for compiler in ["rustc", "flux"] {
            let mut command = if compiler == "rustc" {
                let mut c = Command::new("rustc");
                c.args(["--edition=2024", "--emit=metadata", "-o"])
                    .arg(root.join(format!("{name}.rmeta")));
                c
            } else {
                let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
                c.arg("mir");
                c
            };
            let output = support::process::run(
                command.arg(&source),
                &root,
                &format!("{name}-{compiler}"),
                Duration::from_secs(30),
            )
            .completed();
            assert_eq!(
                output.status.success(),
                expected,
                "{name} {compiler}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

#[test]
fn once_cell_panic_keeps_valid_storage() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/backend-std/host-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let cell = include_str!("../../../library/core/src/cell.rs");
    let implementation = &cell[cell.find("/// A value initialized at most once").unwrap()..];
    let driver = r#"
use std::cell::UnsafeCell;
use std::fmt::{self,Debug,Formatter};
fn panic(message:&str)->!{panic!("{message}")}
fn main(){
    use std::panic::{catch_unwind,AssertUnwindSafe};
    std::panic::set_hook(Box::new(|_|{}));
    let c = OnceCell::<String>::new();
    assert!(catch_unwind(AssertUnwindSafe(|| c.get_or_init(||panic!("init")))).is_err());
    assert!(c.get().is_none());
    assert_eq!(c.get_or_init(||"retry".into()),"retry");
    let c = OnceCell::<String>::new();
    assert!(catch_unwind(AssertUnwindSafe(||c.get_or_init(||{
        c.set("inner".into()).unwrap(); "outer".into()
    }))).is_err());
    assert_eq!(c.get().unwrap(),"inner");
}
"#;
    let source = root.join("host.rs");
    std::fs::write(&source, format!("{implementation}\n{driver}")).unwrap();
    let executable = root.join("host");
    let output = support::process::run(
        Command::new("rustc")
            .args(["--edition=2024", "-O"])
            .arg(&source)
            .arg("-o")
            .arg(&executable),
        &root,
        "host-build",
        Duration::from_secs(30),
    )
    .completed();
    support::oracle::success(&output, "host build");
    let output = support::process::run(
        &mut Command::new(&executable),
        &root,
        "host-run",
        Duration::from_secs(30),
    )
    .completed();
    support::oracle::success(&output, "host unwind");
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn current_dir_preserves_long_and_supported_path_bytes() {
    use std::os::unix::ffi::OsStringExt;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/backend-std/cwd-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut deep = root.clone();
    for _ in 0..8 {
        deep.push("long-directory-component-0123456789");
    }
    std::fs::create_dir_all(&deep).unwrap();
    let raw = deep.join(std::ffi::OsString::from_vec(b"non-unicode-\xff".to_vec()));
    let non_unicode = match std::fs::create_dir_all(&raw) {
        Ok(()) => {
            deep = raw;
            true
        }
        Err(error) if error.raw_os_error() == Some(92) => {
            // This APFS volume rejects invalid UTF-8 filenames (EILSEQ).
            deep.push("unicode-β");
            std::fs::create_dir_all(&deep).unwrap();
            false
        }
        Err(error) => panic!("cannot create path fixture: {error}"),
    };
    let source = root.join("cwd.rs");
    std::fs::write(
        &source,
        r#"
fn main() {
    let cwd=std::env::current_dir().unwrap();
    assert_eq!(cwd,std::fs::canonicalize(".").unwrap());
    assert!(cwd.as_os_str().as_encoded_bytes().len()>256);
    assert_eq!(cwd.to_str().is_none(),NON_UNICODE);
    println!("long cwd bytes preserved");
}
"#
        .replace("NON_UNICODE", if non_unicode { "true" } else { "false" }),
    )
    .unwrap();
    for compiler in ["rustc", "flux", "flux-opt"] {
        let executable = root.join(compiler);
        let mut command = if compiler == "rustc" {
            let mut c = Command::new("rustc");
            c.arg("--edition=2024");
            c
        } else {
            let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
            c.arg("build");
            c
        };
        command.arg(&source).arg("-o").arg(&executable);
        if compiler == "flux-opt" {
            command.arg("-O");
        }
        let output = support::process::run(
            &mut command,
            &root,
            &format!("cwd-build-{compiler}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "cwd build");
        let output = support::process::run(
            Command::new(&executable).current_dir(&deep),
            &root,
            &format!("cwd-run-{compiler}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "cwd run");
        assert_eq!(output.stdout, b"long cwd bytes preserved\n");
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn current_dir_returns_error_for_deleted_directory() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/backend-std/deleted-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("deleted.rs");
    std::fs::write(
        &source,
        r#"
unsafe extern "C" { fn chdir(path:*const u8)->i32;fn rmdir(path:*const u8)->i32; }
fn main(){
    unsafe { assert_eq!(chdir(b"gone\0".as_ptr()),0); assert_eq!(rmdir(b"../gone\0".as_ptr()),0); }
    let error=std::env::current_dir().unwrap_err();
    assert_eq!(error.raw_os_error(),Some(2));
    println!("deleted cwd error preserved");
}
"#,
    )
    .unwrap();
    for compiler in ["rustc", "flux", "flux-opt"] {
        let executable = root.join(compiler);
        let mut command = if compiler == "rustc" {
            let mut c = Command::new("rustc");
            c.arg("--edition=2024");
            c
        } else {
            let mut c = Command::new(env!("CARGO_BIN_EXE_flux"));
            c.arg("build");
            c
        };
        command.arg(&source).arg("-o").arg(&executable);
        if compiler == "flux-opt" {
            command.arg("-O");
        }
        let output = support::process::run(
            &mut command,
            &root,
            &format!("deleted-build-{compiler}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "deleted cwd build");
        let work = root.join(format!("work-{compiler}"));
        std::fs::create_dir_all(work.join("gone")).unwrap();
        let output = support::process::run(
            Command::new(&executable).current_dir(&work),
            &root,
            &format!("deleted-run-{compiler}"),
            Duration::from_secs(30),
        )
        .completed();
        support::oracle::success(&output, "deleted cwd run");
        assert_eq!(output.stdout, b"deleted cwd error preserved\n");
    }
}
