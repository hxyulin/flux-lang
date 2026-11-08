//! Native archive changes invalidate executable reuse without rechecking Rust libraries.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;
use std::{
    path::Path,
    process::{Command, Output},
    time::Duration,
};

struct Fixture(support::scratch::Scratch);
impl Fixture {
    fn new() -> Self {
        Self(support::scratch::Scratch::new(
            std::env::temp_dir().join("flux-native-link-cache"),
        ))
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn build(&self) -> Output {
        support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args(["build", "--offline", "--manifest-path"])
                .arg(self.path().join("Cargo.toml")),
            self.path(),
            "build",
            Duration::from_secs(120),
        )
        .completed()
    }
    fn success(&self) -> String {
        let output = self.build();
        support::oracle::success(&output, "native linked build");
        String::from_utf8(output.stderr).unwrap()
    }
    fn run(&self, expected: &str) {
        let output = Command::new(self.path().join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, "native linked executable");
        assert_eq!(output.stdout, expected.as_bytes());
    }
    fn archive(&self, directory: &str, value: i32) {
        let dir = self.path().join(directory);
        std::fs::create_dir_all(&dir).unwrap();
        self.write(&format!("{directory}/value.c"), &format!("extern int rust_part(void); int native_value(void) {{ return {value} + rust_part(); }}\n"));
        for (tool, args) in [
            ("cc", vec!["-c", "value.c", "-o", "value.o"]),
            ("ar", vec!["crs", "libfixture.a", "value.o"]),
        ] {
            let output = Command::new(tool)
                .args(args)
                .current_dir(&dir)
                .output()
                .unwrap();
            support::oracle::success(&output, tool);
        }
    }
}

#[test]
fn native_archives_are_tracked_through_dependency_artifacts() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','helper']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nhelper={path='../helper'}\n");
    f.write(
        "app/src/main.rs",
        "fn main(){println!(\"{}\",helper::value());}\n",
    );
    f.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write(
        "helper/src/lib.rs",
        r#"
#[link(name="fixture",kind="static")]
unsafe extern "C" { fn native_value()->i32; }
#[unsafe(no_mangle)]
pub extern "C" fn rust_part()->i32 { 4 }
pub fn value()->i32 {
    // SAFETY: the fixture archive exports this signature.
    unsafe { native_value() }
}
"#,
    );
    f.write(
        "helper/build.rs",
        r#"
fn main(){
    let root=std::env::var("CARGO_MANIFEST_DIR").unwrap();
    println!("cargo::rustc-link-search=native={root}/../first");
    println!("cargo::rustc-link-search=native={root}/../second");
    println!("cargo::rustc-link-lib=static=fixture");
    let runs=std::fs::read_to_string("runs.txt").unwrap_or_default();
    std::fs::write("runs.txt",format!("{runs}x")).unwrap();
}
"#,
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n");
    std::fs::create_dir_all(f.path().join("first")).unwrap();
    f.archive("second", 13);
    let first = f.success();
    assert!(
        first.contains("Compiling helper") && first.contains("Compiling app"),
        "{first}"
    );
    f.run("17\n");
    let fresh = f.success();
    assert!(
        fresh.contains("Fresh helper") && fresh.contains("Fresh app"),
        "{fresh}"
    );
    assert_eq!(
        std::fs::read_to_string(f.path().join("helper/runs.txt")).unwrap(),
        "xx"
    );
    let archive = f.path().join("second/libfixture.a");
    let modified = std::fs::metadata(&archive).unwrap().modified().unwrap();
    f.archive("second", 17);
    std::fs::File::open(&archive)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let changed = f.success();
    assert!(
        changed.contains("Fresh helper") && changed.contains("Compiling app"),
        "{changed}"
    );
    assert!(!changed.contains("Reused app after expansion"), "{changed}");
    f.run("21\n");
    f.archive("first", 19);
    let shadow = f.success();
    assert!(
        shadow.contains("Fresh helper") && shadow.contains("Compiling app"),
        "{shadow}"
    );
    f.run("23\n");
    f.archive("third", 23);
    let first_archive = f.path().join("first/libfixture.a");
    std::fs::remove_file(&first_archive).unwrap();
    std::os::unix::fs::symlink(f.path().join("third/libfixture.a"), &first_archive).unwrap();
    f.success();
    f.run("27\n");
    f.archive("fourth", 29);
    std::fs::remove_file(&first_archive).unwrap();
    std::os::unix::fs::symlink(f.path().join("fourth/libfixture.a"), &first_archive).unwrap();
    f.success();
    f.run("33\n");
    std::fs::remove_file(&first_archive).unwrap();
    f.success();
    f.run("21\n");
    std::fs::remove_file(&archive).unwrap();
    let missing = f.build();
    assert!(
        !missing.status.success(),
        "missing native library must not reuse an executable"
    );
}
