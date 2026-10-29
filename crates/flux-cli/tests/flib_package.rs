//! Package libraries use exact snapshots and retain fresh artifacts.
mod flib_support;
use std::process::Command;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn package_reuses_fresh_libraries_and_rebuilds_transitive_includes() {
    use std::os::unix::fs::MetadataExt;
    let s = Scratch::new(std::env::temp_dir().join("flux-flib-package"));
    let write = |path: &str, text: &str| {
        let path = s.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "Cargo.toml",
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\na={path='a'}\n",
    );
    write("src/main.rs", "fn main() { println!(\"{}\", a::value()); }");
    write(
        "a/Cargo.toml",
        "[package]\nname='a'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nb={path='../b'}\n",
    );
    write(
        "a/src/lib.rs",
        "pub fn value() -> &'static str { b::VALUE }",
    );
    write(
        "b/Cargo.toml",
        "[package]\nname='b'\nversion='0.1.0'\nedition='2024'\n",
    );
    write(
        "b/src/lib.rs",
        "pub const VALUE: &str = include_str!(\"value.txt\");",
    );
    write("b/src/value.txt", "first");
    let build = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(s.path())
            .args(["package", "build", "--manifest-path", "Cargo.toml"]);
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
                command.env_remove(key);
            }
        }
        let output = flib_support::run(&mut command, "package");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    let run = || {
        let output = flib_support::run(
            &mut Command::new(s.path().join("target/flux/app")),
            "package-run",
        );
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    build();
    assert_eq!(run(), "first\n");
    let a = s.path().join("target/flux/deps/a-1.flib");
    let b = s.path().join("target/flux/deps/b-0.flib");
    let before = (
        std::fs::metadata(&a).unwrap().ino(),
        std::fs::metadata(&b).unwrap().ino(),
    );
    build();
    assert_eq!(
        before,
        (
            std::fs::metadata(&a).unwrap().ino(),
            std::fs::metadata(&b).unwrap().ino()
        ),
        "fresh libraries must not be rewritten"
    );
    let original_time = std::fs::metadata(s.path().join("b/src/value.txt"))
        .unwrap()
        .modified()
        .unwrap();
    write("b/src/value.txt", "other");
    std::fs::File::options()
        .write(true)
        .open(s.path().join("b/src/value.txt"))
        .unwrap()
        .set_modified(original_time)
        .unwrap();
    build();
    assert_eq!(run(), "other\n");
    assert_ne!(
        before.0,
        std::fs::metadata(a).unwrap().ino(),
        "dependent artifact must change"
    );
    assert_ne!(
        before.1,
        std::fs::metadata(b).unwrap().ino(),
        "included bytes must invalidate artifact"
    );
}

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn package_library_target_writes_a_reusable_fluxlib() {
    use std::os::unix::fs::MetadataExt;
    let s = Scratch::new(std::env::temp_dir().join("flux-flib-package-library"));
    std::fs::create_dir(s.path().join("src")).unwrap();
    std::fs::write(
        s.path().join("Cargo.toml"),
        "[package]\nname='library'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(s.path().join("src/lib.rs"), "pub fn answer() -> u32 { 42 }").unwrap();
    let build = || {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command.current_dir(s.path()).args([
            "package",
            "build",
            "--lib",
            "--manifest-path",
            "Cargo.toml",
        ]);
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
                command.env_remove(key);
            }
        }
        let output = flib_support::run(&mut command, "package-library");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    build();
    let artifact = s.path().join("target/flux/deps/library.flib");
    let first = std::fs::metadata(&artifact).unwrap().ino();
    build();
    assert_eq!(first, std::fs::metadata(&artifact).unwrap().ino());
    std::fs::write(
        s.path().join("consumer.rs"),
        "fn main() { println!(\"{}\", library::answer()); }",
    )
    .unwrap();
    std::fs::remove_file(s.path().join("src/lib.rs")).unwrap();
    let output = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .current_dir(s.path())
            .args([
                "build",
                "consumer.rs",
                "--extern",
                "library=target/flux/deps/library.flib",
                "-o",
                "consumer",
            ]),
        "library-consumer",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = flib_support::run(
        &mut Command::new(s.path().join("consumer")),
        "library-consumer-run",
    );
    assert!(output.status.success());
    assert_eq!(output.stdout, b"42\n");
}
