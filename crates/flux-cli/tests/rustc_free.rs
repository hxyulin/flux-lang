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
    let output = Command::new(root.join("target/flux/app")).output().unwrap();
    support::oracle::success(&output, "rustc-free executable");
    assert_eq!(output.stdout, b"rustc-free\n");
    assert!(!root.join("toolchain-called").exists());
}
