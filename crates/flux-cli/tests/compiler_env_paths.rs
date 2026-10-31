//! Raw Unix environment, argument and path views match the host Rust library.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;
fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    support::process::run(command, root, label, Duration::from_secs(60)).completed()
}
fn execute(binary: &Path, root: &Path, temp: Option<&OsStr>, label: &str) -> Output {
    let mut command = Command::new(binary);
    command.args([
        OsString::from("first"),
        OsString::from_vec(b"a\xffb".to_vec()),
        OsString::from("last"),
    ]);
    command
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("FLUX_ENV_PRESENT", "hello β")
        .env("FLUX_ENV_EMPTY", "")
        .env("FLUX_ENV_RAW", OsStr::from_bytes(b"raw\xff"))
        .env(OsStr::from_bytes(b"FLUX_ENV_\xff_KEY"), "raw-key")
        .env("FLUX_ENV_FIXTURE", root.join("files"));
    if let Some(temp) = temp {
        command.env("TMPDIR", temp);
    }
    run(&mut command, root, label)
}
#[test]
fn compiler_env_paths_match_rustc() {
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/compiler-env-paths/{}", std::process::id()));
    let files = root.join("files");
    std::fs::create_dir_all(files.join("dir")).unwrap();
    std::fs::write(files.join("file"), b"x").unwrap();
    for (link, target) in [("link", "file"), ("broken", "missing"), ("loop", "loop")] {
        let path = files.join(link);
        if std::fs::symlink_metadata(&path).is_err() {
            std::os::unix::fs::symlink(target, path).unwrap();
        }
    }
    let source = root.join("consumer.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/compiler_env_paths/consumer.rs.txt"),
    )
    .unwrap();
    let reference = root.join("rustc");
    let built = run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&source)
            .arg("-o")
            .arg(&reference),
        &root,
        "rustc-build",
    );
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            command.arg("-O");
        }
        let built = run(&mut command, &root, &format!("flux-build-{optimized}"));
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        for (case, temp) in [
            ("unset", None),
            ("empty", Some(OsStr::new(""))),
            ("raw", Some(OsStr::from_bytes(b"/tmp/\xff-path"))),
        ] {
            let expected = execute(
                &reference,
                &root,
                temp,
                &format!("rustc-{case}-{optimized}"),
            );
            assert!(
                expected.status.success(),
                "{}",
                String::from_utf8_lossy(&expected.stderr)
            );
            let actual = execute(
                &executable,
                &root,
                temp,
                &format!("flux-{case}-{optimized}"),
            );
            assert_eq!(actual.status.code(), expected.status.code(), "{case}");
            assert_eq!(actual.stdout, expected.stdout, "{case}");
            assert_eq!(actual.stderr, expected.stderr, "{case}");
        }
    }
}

#[test]
fn env_path_contracts_match_rustc() {
    let cases = [
        (
            "args_owned",
            true,
            "fn value()->std::ffi::OsString{std::env::args_os().nth(1).unwrap()}fn main(){}",
        ),
        (
            "args_double_exact",
            true,
            "fn take<I:DoubleEndedIterator+ExactSizeIterator>(_:I){}fn main(){take(std::env::args_os());}",
        ),
        (
            "component_bounds",
            true,
            "fn take<T:Send+Sync+Clone>(_:T){}fn main(){take(std::path::Path::new(\"a/b\").components());take(std::path::Path::new(\"a/b\").ancestors());}",
        ),
        (
            "prefix_pattern",
            true,
            "fn main(){match std::path::Path::new(\"a\").components().next(){Some(std::path::Component::Prefix(p))=>{let _=p.as_os_str();},_=>{}}}",
        ),
        (
            "args_not_send",
            false,
            "fn take<T:Send>(_:T){}fn main(){take(std::env::args_os());}",
        ),
        (
            "vars_not_sync",
            false,
            "fn take<T:Sync>(_:T){}fn main(){take(std::env::vars_os());}",
        ),
        (
            "args_not_clone",
            false,
            "fn main(){let _=std::env::args_os().clone();}",
        ),
        (
            "vars_not_double",
            false,
            "fn main(){let _=std::env::vars_os().next_back();}",
        ),
        (
            "vars_not_exact",
            false,
            "fn take<T:ExactSizeIterator>(_:T){}fn main(){take(std::env::vars_os());}",
        ),
        (
            "components_no_escape",
            false,
            "fn bad()->std::path::Components<'static>{let p=std::path::PathBuf::from(\"x\");p.components()}fn main(){}",
        ),
        (
            "stem_no_escape",
            false,
            "fn bad()->&'static std::ffi::OsStr{let p=std::path::PathBuf::from(\"x.rs\");p.file_stem().unwrap()}fn main(){}",
        ),
    ];
    let root = std::env::temp_dir().join(format!("flux-env-path-contracts-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expected, source) in cases {
        let file = root.join(format!("{name}.rs"));
        std::fs::write(&file, source).unwrap();
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
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(
            hir.diagnostics.is_empty(),
            "HIR {name}: {:?}",
            hir.diagnostics
        );
        let typed = flux_mir::check_file(&hir);
        let accepted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(accepted, expected, "Flux {name}: {:?}", typed.diagnostics);
    }
}
