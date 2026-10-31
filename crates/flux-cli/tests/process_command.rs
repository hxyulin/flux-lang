//! Unix child process and pipe ownership contracts match native Rust.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
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
fn process_commands_match_rustc() {
    use std::os::unix::fs::PermissionsExt;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!("out/process-command/{}", std::process::id()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("work")).unwrap();
    let child = root.join("child.rs");
    std::fs::write(
        &child,
        include_str!("fixtures/process_command/child.rs.txt"),
    )
    .unwrap();
    let child_binary = root.join("child");
    success(&run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&child)
            .arg("-o")
            .arg(&child_binary),
        &root,
        "child-build",
    ));
    let helper = root.join("bin/helper");
    if !helper.exists() {
        std::os::unix::fs::symlink(&child_binary, &helper).unwrap();
    }
    std::fs::write(root.join("denied"), b"not executable").unwrap();
    std::fs::set_permissions(root.join("denied"), std::fs::Permissions::from_mode(0o600)).unwrap();
    std::fs::write(root.join("bad-format"), b"not a machine executable\n").unwrap();
    std::fs::set_permissions(
        root.join("bad-format"),
        std::fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    std::fs::write(root.join("script"), b"printf '%s|%s\\n' \"$0\" \"$1\"\n").unwrap();
    std::fs::set_permissions(root.join("script"), std::fs::Permissions::from_mode(0o700)).unwrap();
    let source = root.join("consumer.rs");
    std::fs::write(
        &source,
        include_str!("fixtures/process_command/consumer.rs.txt"),
    )
    .unwrap();
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
    let expected = run(
        Command::new(&reference)
            .env("FLUX_PROCESS_CHILD", &child_binary)
            .env("FLUX_PROCESS_ROOT", &root)
            .env("FLUX_PROCESS_PARENT", "keep"),
        &root,
        "reference-run",
    );
    success(&expected);
    let closed_source = root.join("closed.rs");
    std::fs::write(
        &closed_source,
        include_str!("fixtures/process_command/closed.rs.txt"),
    )
    .unwrap();
    let closed_reference = root.join("closed-reference");
    success(&run(
        Command::new("rustc")
            .args(["--edition=2024", "-C", "panic=abort"])
            .arg(&closed_source)
            .arg("-o")
            .arg(&closed_reference),
        &root,
        "closed-rustc-build",
    ));
    let closed_expected = run(
        Command::new("/bin/sh")
            .args(["-c", "ulimit -n 32; exec \"$1\"", "sh"])
            .arg(&closed_reference)
            .env("FLUX_PROCESS_CHILD", &child_binary)
            .env("FLUX_PROCESS_ROOT", &root),
        &root,
        "closed-reference",
    );
    success(&closed_expected);
    for optimized in [false, true] {
        let executable = root.join(format!("flux-{optimized}"));
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        cmd.arg("build").arg(&source).arg("-o").arg(&executable);
        if optimized {
            cmd.arg("-O");
        }
        success(&run(&mut cmd, &root, &format!("build-{optimized}")));
        let actual = run(
            Command::new(&executable)
                .env("FLUX_PROCESS_CHILD", &child_binary)
                .env("FLUX_PROCESS_ROOT", &root)
                .env("FLUX_PROCESS_PARENT", "keep"),
            &root,
            &format!("run-{optimized}"),
        );
        success(&actual);
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
        let closed = root.join(format!("closed-flux-{optimized}"));
        let mut build = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        build
            .arg("build")
            .arg(&closed_source)
            .arg("-o")
            .arg(&closed);
        if optimized {
            build.arg("-O");
        }
        success(&run(
            &mut build,
            &root,
            &format!("closed-build-{optimized}"),
        ));
        let actual = run(
            Command::new("/bin/sh")
                .args(["-c", "ulimit -n 32; exec \"$1\"", "sh"])
                .arg(&closed)
                .env("FLUX_PROCESS_CHILD", &child_binary)
                .env("FLUX_PROCESS_ROOT", &root),
            &root,
            &format!("closed-run-{optimized}"),
        );
        success(&actual);
        assert_eq!(actual.stdout, closed_expected.stdout);
        assert_eq!(actual.stderr, closed_expected.stderr);
    }
}

#[test]
fn process_interface_contracts_match_rustc() {
    let cases = [
        (
            "send_sync",
            true,
            "fn yes<T:Send+Sync>(){}fn main(){yes::<std::process::Command>();yes::<std::process::Child>();yes::<std::process::ChildStdin>();yes::<std::process::ChildStdout>();yes::<std::process::ChildStderr>();yes::<std::process::Stdio>();}",
        ),
        (
            "pipe_bounds",
            true,
            "fn read<T:std::io::Read>(){}fn write<T:std::io::Write>(){}fn main(){read::<std::process::ChildStdout>();read::<std::process::ChildStderr>();write::<std::process::ChildStdin>();}",
        ),
        (
            "debug_bounds",
            true,
            "fn yes<T:std::fmt::Debug>(){}fn main(){yes::<std::process::Child>();yes::<std::process::ChildStdin>();yes::<std::process::ChildStdout>();yes::<std::process::ChildStderr>();yes::<std::process::Stdio>();yes::<std::process::Output>();}",
        ),
        (
            "command_not_clone",
            false,
            "fn main(){let _=std::process::Command::new(\"true\").clone();}",
        ),
        (
            "child_not_clone",
            false,
            "fn main(){let _=std::process::Command::new(\"true\").spawn().unwrap().clone();}",
        ),
        (
            "stdin_not_read",
            false,
            "fn yes<T:std::io::Read>(){}fn main(){yes::<std::process::ChildStdin>();}",
        ),
        (
            "stdout_not_write",
            false,
            "fn yes<T:std::io::Write>(){}fn main(){yes::<std::process::ChildStdout>();}",
        ),
        (
            "output_consumes_child",
            false,
            "fn main(){let mut c=std::process::Command::new(\"true\").spawn().unwrap();let _=c.wait_with_output();let _=c.wait();}",
        ),
        (
            "exit_status_ext_sealed",
            false,
            "struct Mine;impl std::os::unix::process::ExitStatusExt for Mine{fn from_raw(_:i32)->Self{Mine}fn into_raw(self)->i32{0}fn signal(&self)->Option<i32>{None}fn core_dumped(&self)->bool{false}fn stopped_signal(&self)->Option<i32>{None}fn continued(&self)->bool{false}}fn main(){}",
        ),
    ];
    let root = std::env::temp_dir().join(format!("flux-process-interface-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (name, expected, source) in cases {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        let out = Command::new("rustc")
            .args(["--edition=2024", "--emit=metadata"])
            .arg(&path)
            .arg("-o")
            .arg(root.join("oracle.rmeta"))
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            expected,
            "rustc {name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let (_, _, hir) = flux_hir::lower_text("main.rs", source);
        assert!(
            hir.diagnostics.is_empty(),
            "{name} HIR: {:?}",
            hir.diagnostics
        );
        let typed = flux_mir::check_file(&hir);
        let admitted = typed.diagnostics.is_empty()
            && flux_mir::build_file(&hir, &typed).diagnostics.is_empty();
        assert_eq!(admitted, expected, "Flux {name}: {:?}", typed.diagnostics);
    }
}
