//! Package libraries use exact snapshots and retain fresh artifacts.
mod flib_support;
use std::process::Command;

#[path = "../../../tests/support/scratch.rs"]
mod scratch;
use scratch::Scratch;

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
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
    write(
        "src/main.rs",
        "fn count<S:a::Values>() -> u32 { S::values().sum() } fn main() { let mut data=[3u32,8]; let mapped=a::map_offset(data.as_mut_ptr(),4); assert_eq!(unsafe {*mapped},8); use core::future::Future; let mut future=core::pin::pin!(a::suspended()); let mut cx=core::task::Context::from_waker(core::task::Waker::noop()); assert!(future.as_mut().poll(&mut cx).is_pending()); assert_eq!(future.as_mut().poll(&mut cx),core::task::Poll::Ready(16)); let value=42u32; let mut borrowed=core::pin::pin!(a::borrowed(&value)); assert!(borrowed.as_mut().poll(&mut cx).is_pending()); assert_eq!(borrowed.as_mut().poll(&mut cx),core::task::Poll::Ready(&42)); assert_eq!(count::<a::Counter>(), 6); assert_eq!(a::offset::<u64>(), 8); println!(\"{}\", a::value()); }",
    );
    write(
        "a/Cargo.toml",
        "[package]\nname='a'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nb={path='../b'}\n",
    );
    write(
        "a/src/lib.rs",
        "struct Tick(bool); impl core::future::Future for Tick { type Output=u32; fn poll(self:core::pin::Pin<&mut Self>,_: &mut core::task::Context<'_>)->core::task::Poll<u32> { let this=self.get_mut(); if this.0 {this.0=false;core::task::Poll::Pending} else {core::task::Poll::Ready(7)} } } pub async fn suspended()->u32 {Tick(true).await+9} pub async fn borrowed<T>(value:&T)->&T {Tick(true).await;value} pub fn map_offset<T>(pointer:*mut T,bytes:usize)->*mut T {pointer.map_addr(|address|address+bytes)} pub trait Values { fn values() -> impl Iterator<Item=u32>; } pub struct Counter; impl Values for Counter { fn values() -> impl Iterator<Item=u32> { 1..4 } } pub fn value() -> &'static str { b::VALUE } #[repr(C)] pub struct Pair<T> { pub tag: u8, pub value: T } pub fn offset<T>() -> usize { core::mem::offset_of!(Pair<T>, value) }",
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
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
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

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[test]
fn package_versions_and_manifests_distinguish_the_same_source() {
    use std::os::unix::fs::MetadataExt;
    let scratch = Scratch::new(std::env::temp_dir().join("flux-flib-package-identity"));
    let root = scratch.path();
    std::fs::write(
        root.join("shared.rs"),
        "pub struct Marker; pub fn answer() -> u32 { 42 }",
    )
    .unwrap();
    let manifest_text = |version| {
        format!(
            "[package]\nname='library'\nversion='{version}'\nedition='2024'\n[lib]\npath='../shared.rs'\n"
        )
    };
    for directory in ["first", "second"] {
        std::fs::create_dir(root.join(directory)).unwrap();
        std::fs::write(
            root.join(directory).join("Cargo.toml"),
            manifest_text("0.1.0"),
        )
        .unwrap();
    }
    let build = |directory: &str| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .current_dir(root)
            .args(["package", "build", "--lib", "--manifest-path"])
            .arg(root.join(directory).join("Cargo.toml"));
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
        let output = flib_support::run(&mut command, "package-identity");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        root.join(directory).join("target/flux/deps/library.flib")
    };
    let compiler = flux_hir::flib::digest(&std::fs::read(env!("CARGO_BIN_EXE_fluxc")).unwrap());
    let hash = |bytes: &[u8]| {
        use flux_hir::flib::{Decode, Decoder};
        let archive = flux_cli::flib::Archive::decode(bytes).unwrap();
        let metadata = flux_cli::flib::Metadata::decode(&archive.metadata, compiler).unwrap();
        let identity = metadata
            .sections
            .iter()
            .find(|section| section.tag == 1)
            .unwrap();
        let mut decoder = Decoder::new(&identity.bytes);
        let _name = String::decode(&mut decoder).unwrap();
        Vec::<u8>::decode(&mut decoder).unwrap()
    };
    let first = build("first");
    let original = std::fs::read(&first).unwrap();
    let original_hash = hash(&original);
    let inode = std::fs::metadata(&first).unwrap().ino();
    build("first");
    assert_eq!(
        inode,
        std::fs::metadata(&first).unwrap().ino(),
        "unchanged package must reuse its artifact"
    );
    std::fs::write(root.join("original.flib"), original).unwrap();
    std::fs::write(root.join("first/Cargo.toml"), manifest_text("0.2.0")).unwrap();
    build("first");
    assert_ne!(
        inode,
        std::fs::metadata(&first).unwrap().ino(),
        "version-only edit must rebuild"
    );
    let versioned = std::fs::read(&first).unwrap();
    let versioned_hash = hash(&versioned);
    assert_ne!(original_hash, versioned_hash);
    let inode = std::fs::metadata(&first).unwrap().ino();
    build("first");
    assert_eq!(inode, std::fs::metadata(&first).unwrap().ino());
    std::fs::write(root.join("second/Cargo.toml"), manifest_text("0.2.0")).unwrap();
    let second = build("second");
    let other = std::fs::read(&second).unwrap();
    let other_hash = hash(&other);
    assert_ne!(
        versioned_hash, other_hash,
        "distinct manifests sharing source must keep distinct identities"
    );
    std::fs::write(
        root.join("consumer.rs"),
        r#"
fn main() {
    use std::any::TypeId;
    assert!(TypeId::of::<old::Marker>() != TypeId::of::<new::Marker>());
    assert!(TypeId::of::<new::Marker>() != TypeId::of::<other::Marker>());
    assert!(TypeId::of::<old::Marker>() != TypeId::of::<other::Marker>());
    println!("{} {} {}", old::answer(), new::answer(), other::answer());
}
"#,
    )
    .unwrap();
    std::fs::remove_file(root.join("shared.rs")).unwrap();
    let executable = root.join("consumer");
    let output = flib_support::run(
        Command::new(env!("CARGO_BIN_EXE_fluxc"))
            .current_dir(root)
            .args([
                "build",
                "consumer.rs",
                "--extern",
                "old=original.flib",
                "--extern",
                "new=first/target/flux/deps/library.flib",
                "--extern",
                "other=second/target/flux/deps/library.flib",
                "-o",
            ])
            .arg(&executable),
        "package-identity-consumer",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let output = flib_support::run(&mut Command::new(executable), "package-identity-run");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"42 42 42\n");
}
