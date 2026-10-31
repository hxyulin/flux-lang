//! Native Linux arguments, recursive removal and bounded x86 assembly.
#![cfg(all(target_os = "linux", target_arch = "x86_64"))]
mod support;
#[test]
fn x86_inline_assembly_matches_rustc() {
    support::oracle::compare_fixture("x86_inline_asm");
}

#[test]
fn recursive_removal_matches_rustc() {
    support::oracle::compare_fixture("remove_dir_all");
}

#[test]
fn trailing_slash_on_symlink_root_preserves_target() {
    use std::process::Command;
    use std::time::Duration;
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let root = repo.join(format!(
        "out/linux-platform-trailing/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("trailing.rs");
    std::fs::write(
        &source,
        r#"
fn main() {
    let root = std::env::temp_dir().join(format!("flux-trailing-{}", std::process::id()));
    std::fs::create_dir_all(root.join("outside")).unwrap();
    std::fs::write(root.join("outside/keep"), b"kept").unwrap();
    std::os::unix::fs::symlink("outside", root.join("link")).unwrap();
    assert_eq!(std::fs::remove_dir_all(root.join("link/")).unwrap_err().raw_os_error(), Some(20));
    assert_eq!(std::fs::read(root.join("outside/keep")).unwrap(), b"kept");
    assert!(std::fs::symlink_metadata(root.join("link")).unwrap().file_type().is_symlink());
    std::fs::remove_dir_all(&root).unwrap();
}
"#,
    )
    .unwrap();
    for optimized in [false, true] {
        let binary = root.join(format!("flux-{optimized}"));
        let mut command = Command::new(env!("CARGO_BIN_EXE_fluxc"));
        command.arg("build").arg(&source).arg("-o").arg(&binary);
        if optimized {
            command.arg("-O");
        }
        support::oracle::success(
            &support::process::run(&mut command, &root, "build", Duration::from_secs(60))
                .completed(),
            "build trailing symlink test",
        );
        support::oracle::success(
            &support::process::run(
                &mut Command::new(&binary),
                &root,
                "run",
                Duration::from_secs(60),
            )
            .completed(),
            "preserve trailing symlink target",
        );
    }
}

#[test]
fn package_entry_receives_raw_arguments() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;
    let root = std::env::temp_dir().join(format!("flux-platform-args-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='args-probe'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("arguments/src")).unwrap();
    std::fs::write(
        root.join("arguments/Cargo.toml"),
        "[package]\nname='arguments'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("arguments/src/lib.rs"),
        "pub fn values()->Vec<std::ffi::OsString>{std::env::args_os().collect()}",
    )
    .unwrap();
    std::fs::write(root.join("Cargo.toml"), "[package]\nname='args-probe'\nversion='0.1.0'\nedition='2024'\n[dependencies]\narguments={path='arguments'}\n").unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        r#"
use std::os::unix::ffi::OsStrExt;
fn main() {
    let mut arguments=arguments::values().into_iter();
    assert_eq!(arguments.len(), 3);
    assert_eq!(arguments.next().unwrap().as_bytes(), b"custom\xff-name");
    assert_eq!(arguments.next_back().unwrap().as_bytes(), b"");
    assert_eq!(arguments.next().unwrap().as_bytes(), b"raw\xfe-argument");
    assert!(arguments.next().is_none());
    assert_eq!(std::env::args_os().len(), 3);
}
"#,
    )
    .unwrap();
    for optimized in [false, true] {
        let mut build = Command::new(env!("CARGO_BIN_EXE_flux"));
        build
            .args(["package", "build", "--manifest-path"])
            .arg(root.join("Cargo.toml"));
        build.env("CARGO_HOME", root.join("cargo-home"));
        for (key, _) in std::env::vars_os() {
            let name = key.to_string_lossy();
            if name.contains("RUSTFLAGS")
                || name.contains("RUSTDOCFLAGS")
                || name.starts_with("CARGO_BUILD_")
                || name.starts_with("CARGO_TARGET_")
                || name.starts_with("CARGO_PROFILE_")
                || name.starts_with("CARGO_UNSTABLE_")
                || name == "CARGO_INCREMENTAL"
            {
                build.env_remove(key);
            }
        }
        if optimized {
            build.arg("-O");
        }
        support::oracle::success(
            &support::process::run(&mut build, &root, "build", Duration::from_secs(60)).completed(),
            "build package argument test",
        );
        let mut command = Command::new(root.join("target/flux/args-probe"));
        command
            .arg0(OsStr::from_bytes(b"custom\xff-name"))
            .arg(OsStr::from_bytes(b"raw\xfe-argument"))
            .arg("");
        support::oracle::success(
            &support::process::run(&mut command, &root, "run", Duration::from_secs(60)).completed(),
            "capture raw package arguments",
        );
    }
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn x86_register_contracts_match_rustc() {
    use std::process::Command;
    use std::time::Duration;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/linux-platform-asm-contracts/{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    for (name, source) in [
        (
            "wrong_modifier",
            "fn main(){let mut x=1u32;unsafe{core::arch::asm!(\"/*{0:w}*/\",inout(reg)x);}}",
        ),
        (
            "byte_in_reg",
            "fn main(){unsafe{core::arch::asm!(\"/*{0}*/\",in(reg)1u8);}}",
        ),
        (
            "float_in_reg",
            "fn main(){unsafe{core::arch::asm!(\"/*{0}*/\",in(reg)1.0f64);}}",
        ),
        (
            "safe_asm",
            "fn main(){let mut x=1u64;core::arch::asm!(\"/*{0:r}*/\",inout(reg)x);}",
        ),
    ] {
        let path = root.join(format!("{name}.rs"));
        std::fs::write(&path, source).unwrap();
        for compiler in ["rustc", env!("CARGO_BIN_EXE_fluxc")] {
            let mut command = Command::new(compiler);
            if compiler == "rustc" {
                command.arg("--edition=2024");
            } else {
                command.arg("build");
            }
            let output = support::process::run(
                command.arg(&path).arg("-o").arg(root.join("program")),
                &root,
                name,
                Duration::from_secs(60),
            )
            .completed();
            assert_eq!(
                output.status.success(),
                name == "float_in_reg",
                "{compiler} {name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
