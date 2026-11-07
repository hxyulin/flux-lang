//! Build-script link directives and `#[link]` kinds reach the final link,
//! as Cargo hands them to rustc: a static archive a build script compiles
//! with `cc` and `ar`, frameworks, and the binary's own link arguments.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn write(root: &Path, file: &str, text: &str) {
    let path = root.join(file);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(180)).completed();
    support::oracle::success(&output, label);
    output
}

#[test]
fn build_script_archives_and_frameworks_link_like_cargo() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/native-link/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    write(
        &root,
        "Cargo.toml",
        "[workspace]\nmembers=[\"app\",\"native\"]\nresolver=\"3\"\n",
    );
    write(
        &root,
        "native/Cargo.toml",
        "[package]\nname=\"native\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    write(
        &root,
        "native/c/add.c",
        "static int counter = 40;\nint flux_add(int a, int b) { return a + b; }\nint flux_counter(void) { return ++counter; }\n",
    );
    write(
        &root,
        "native/build.rs",
        r#"
use std::process::Command;
fn main() {
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let object = out.join("add.o");
    let compiled = Command::new("cc").args(["-c", "c/add.c", "-o"]).arg(&object).status().unwrap();
    assert!(compiled.success());
    let archived = Command::new("ar").arg("crs").arg(out.join("libflux_add.a")).arg(&object).status().unwrap();
    assert!(archived.success());
    println!("cargo:rustc-link-search=native={}", out.display());
    println!("cargo:rustc-link-lib=static=flux_add");
    println!("cargo:rustc-link-lib=framework=CoreFoundation");
}
"#,
    );
    write(
        &root,
        "native/src/lib.rs",
        r#"
unsafe extern "C" {
    fn flux_add(a: i32, b: i32) -> i32;
    fn flux_counter() -> i32;
    fn CFStringGetTypeID() -> usize;
}
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecRandomCopyBytes(generator: *const u8, count: usize, bytes: *mut u8) -> i32;
}
pub fn add(a: i32, b: i32) -> i32 {
    unsafe { flux_add(a, b) }
}
pub fn counter() -> i32 {
    unsafe { flux_counter() }
}
pub fn string_type_known() -> bool {
    unsafe { CFStringGetTypeID() != 0 }
}
pub fn random_bytes() -> bool {
    let mut bytes = [0u8; 8];
    unsafe { SecRandomCopyBytes(std::ptr::null(), bytes.len(), bytes.as_mut_ptr()) == 0 }
}
"#,
    );
    write(
        &root,
        "app/Cargo.toml",
        "[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nnative={path=\"../native\"}\n",
    );
    write(
        &root,
        "app/build.rs",
        "fn main() { println!(\"cargo:rustc-link-arg-bins=-lz\"); }\n",
    );
    write(
        &root,
        "app/src/main.rs",
        r#"
unsafe extern "C" {
    fn zlibVersion() -> *const u8;
}
fn main() {
    println!("{}", native::add(2, 3));
    println!("{} {}", native::counter(), native::counter());
    println!("{}", native::string_type_known());
    println!("{}", native::random_bytes());
    println!("{}", unsafe { *zlibVersion() } == b'1');
}
"#,
    );
    let expected = run(
        Command::new("cargo")
            .args(["run", "-q", "-p", "app", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", root.join("cargo-target")),
        &root,
        "cargo",
    );
    assert_eq!(
        String::from_utf8_lossy(&expected.stdout),
        "5\n41 42\ntrue\ntrue\ntrue\n"
    );
    for (label, args) in [("native", &[][..]), ("cargo-plan", &["--cargo"][..])] {
        let _ = std::fs::remove_file(root.join("target/flux/debug/app"));
        run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args(["build", "--manifest-path"])
                .arg(root.join("Cargo.toml"))
                .args(["-p", "app", "--plan"])
                .arg(root.join(format!("{label}.json")))
                .args(args),
            &root,
            &format!("{label}-build"),
        );
        let actual = run(
            &mut Command::new(root.join("target/flux/debug/app")),
            &root,
            &format!("{label}-run"),
        );
        assert_eq!(actual.stdout, expected.stdout, "{label}");
    }
    std::fs::remove_dir_all(&root).unwrap();
}
