//! Native packages must not require an installed Rust toolchain.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use support::scratch::Scratch;

#[test]
fn resolve_plan_build_and_run_without_rustc_or_cargo() {
    check(false);
}

#[test]
fn build_scripts_and_dependencies_compile_without_rustc() {
    check(true);
}

#[test]
fn declared_script_inputs_and_generated_files_control_reuse() {
    let scratch = Scratch::new(std::env::temp_dir().join("flux-script-freshness"));
    let root = scratch.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir(root.join("inputs")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(root.join("inputs/message"), "first").unwrap();
    std::fs::write(
        root.join("build.rs"),
        r#"fn main() {
            println!("cargo::rerun-if-changed=inputs");
            println!("cargo:rerun-if-env-changed=SCRIPT_SUFFIX");
            let value = std::fs::read_to_string("inputs/message").unwrap();
            let suffix = std::env::var("SCRIPT_SUFFIX").unwrap_or(String::new());
            let out = std::env::var("OUT_DIR").unwrap();
            let fail = suffix == "fail";
            std::fs::write(std::path::Path::new(&out).join("padding"), &[b'x'; 8192][..]).unwrap();
            std::fs::write(std::path::Path::new(&out).join("message"), value + &suffix).unwrap();
            if fail {
                std::process::exit(1);
            }
        }"#,
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        r#"fn main() {
            print!("{}", include_str!(concat!(env!("OUT_DIR"), "/message")));
        }"#,
    )
    .unwrap();
    let flux = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(root)
            .env("CARGO_HOME", root.join("cache"))
            .env("RUSTC", root.join("no-rustc"))
            .env_remove("SCRIPT_SUFFIX");
        command
    };
    let lockfile = flux()
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    support::oracle::success(&lockfile, "native build-script lockfile");
    let build = |suffix: Option<&str>, fresh, expected: &[u8]| {
        let mut command = flux();
        command.args(["build", "--offline"]);
        if let Some(suffix) = suffix {
            command.env("SCRIPT_SUFFIX", suffix);
        }
        let output = command.output().unwrap();
        support::oracle::success(&output, "native build-script freshness");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.contains("fresh build script app"), fresh, "{stderr}");
        let output = Command::new(root.join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, "generated value");
        assert_eq!(output.stdout, expected);
    };
    build(None, false, b"first");
    let script = std::fs::read_dir(root.join("target/flux/native-host"))
        .unwrap()
        .flatten()
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("build-app-")
        })
        .unwrap();
    let padding = script.path().join("out/padding").canonicalize().unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    build(None, true, b"first");
    let record: serde_json::Value =
        serde_json::from_slice(&std::fs::read(script.path().join("script-fresh.json")).unwrap())
            .unwrap();
    assert!(record["file_hashes"][padding.to_str().unwrap()].is_object());
    let modified = std::fs::metadata(&padding).unwrap().modified().unwrap();
    std::fs::write(&padding, [b'y'; 8192]).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&padding)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    std::thread::sleep(std::time::Duration::from_secs(2));
    build(None, false, b"first");
    assert_eq!(std::fs::read(&padding).unwrap(), [b'x'; 8192]);
    std::fs::write(root.join("inputs/message"), "second").unwrap();
    build(None, false, b"second");
    build(Some("!"), false, b"second!");
    build(Some("!"), true, b"second!");
    build(None, false, b"second");
    std::fs::remove_file(script.path().join("out/message")).unwrap();
    build(None, false, b"second");
    build(None, true, b"second");
    let failed = flux()
        .args(["build", "--offline"])
        .env("SCRIPT_SUFFIX", "fail")
        .output()
        .unwrap();
    assert!(!failed.status.success());
    assert!(!script.path().join("script-fresh.json").exists());
    build(None, false, b"second");
    build(None, true, b"second");
}

fn check(script: bool) {
    let scratch = Scratch::new(std::env::temp_dir().join("flux-rustc-free"));
    let root = scratch.path();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nresolver='3'\n[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        "fn main(){assert!(cfg!(panic=\"abort\"));println!(\"rustc-free\");}\n",
    )
    .unwrap();
    if script {
        std::fs::create_dir_all(root.join("helper/src")).unwrap();
        let manifest = root.join("Cargo.toml");
        let mut text = std::fs::read_to_string(&manifest).unwrap();
        text.push_str("[build-dependencies]\nhelper={path='helper'}\n");
        std::fs::write(manifest, text).unwrap();
        std::fs::write(
            root.join("helper/Cargo.toml"),
            "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
        )
        .unwrap();
        std::fs::write(
            root.join("helper/src/lib.rs"),
            "pub fn generated()-> &'static str { \"pub const VALUE:usize=42;\" }",
        )
        .unwrap();
        std::fs::write(root.join("build.rs"), r#"fn main(){
            let out=std::env::var("OUT_DIR").unwrap();
            std::fs::write(std::path::Path::new(&out).join("generated.rs"),helper::generated()).unwrap();
            println!("cargo::rustc-cfg=generated");
            println!("cargo::rustc-env=GENERATED=yes");
        }"#).unwrap();
        std::fs::write(root.join("src/main.rs"), r#"
            include!(concat!(env!("OUT_DIR"),"/generated.rs"));
            #[cfg(not(generated))] compile_error!("missing generated cfg");
            fn main(){assert_eq!(VALUE,42);assert_eq!(env!("GENERATED"),"yes");println!("rustc-free");}
        "#).unwrap();
    }
    for name in ["cargo", "rustc"] {
        let path = root.join("tools").join(name);
        std::fs::write(
            &path,
            "#!/bin/sh\necho called > \"$TOOLCHAIN_TRAP\"\nexit 93\n",
        )
        .unwrap();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = std::env::join_paths(
        std::iter::once(root.join("tools"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    for args in [
        vec!["generate-lockfile", "--offline"],
        vec!["plan", "--offline"],
        vec!["build", "--offline"],
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(root)
            .args(args)
            .env("PATH", &path)
            .env("RUSTC", root.join("tools/rustc"))
            .env("CARGO_HOME", root.join("cache"))
            .env("TOOLCHAIN_TRAP", root.join("toolchain-called"))
            .output()
            .unwrap();
        support::oracle::success(&output, "rustc-free native package");
    }
    let output = Command::new(root.join("target/flux/debug/app"))
        .output()
        .unwrap();
    support::oracle::success(&output, "rustc-free executable");
    assert_eq!(output.stdout, b"rustc-free\n");
    assert!(!root.join("toolchain-called").exists());
}

#[test]
fn compiler_probes_check_flux_capabilities_without_a_toolchain() {
    let scratch = Scratch::new(std::env::temp_dir().join("flux-compiler-probes"));
    let root = scratch.path();
    std::fs::write(root.join("valid.rs"), "pub fn value()->usize {42}").unwrap();
    std::fs::write(root.join("invalid.rs"), "pub fn value()->usize {true}").unwrap();
    std::fs::write(
        root.join("span.rs"),
        "extern crate proc_macro; pub fn file(s:proc_macro::Span){s.file();}",
    )
    .unwrap();
    let invoke = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .current_dir(root)
            .args(args)
            .env("FLUX_COMPILER_PROBE", "1")
            .env("PATH", root.join("no-tools"))
            .env("RUSTC", root.join("no-rustc"))
            .output()
            .unwrap()
    };
    let version = invoke(&["--version"]);
    support::oracle::success(&version, "Flux probe version");
    assert!(version.stdout.starts_with(b"rustc 1.85.0 (fluxc "));
    let normal = Command::new(env!("CARGO_BIN_EXE_fluxc"))
        .arg("--version")
        .env_remove("FLUX_COMPILER_PROBE")
        .output()
        .unwrap();
    support::oracle::success(&normal, "normal Flux version");
    assert!(normal.stdout.starts_with(b"fluxc "));
    let cfg = invoke(&["--print", "cfg"]);
    support::oracle::success(&cfg, "Flux probe cfg");
    assert!(
        String::from_utf8(cfg.stdout)
            .unwrap()
            .contains("panic=\"abort\"")
    );
    for (file, succeeds) in [("valid.rs", true), ("invalid.rs", false), ("span.rs", true)] {
        let output = invoke(&[
            "--edition=2024",
            "--crate-name=probe",
            "--crate-type=lib",
            "--emit=dep-info,metadata",
            "--out-dir",
            "output",
            file,
        ]);
        assert_eq!(
            output.status.success(),
            succeeds,
            "{file}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    assert!(!invoke(&["--unsupported"]).status.success());
}
