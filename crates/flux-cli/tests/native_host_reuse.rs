//! Compilation reuse must not skip the execution effects of host tools.
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
            std::env::temp_dir().join("flux-host-reuse"),
        ))
    }
    fn path(&self) -> &Path {
        self.0.path()
    }
    fn write(&self, path: &str, content: &str) {
        let path = self.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
    fn invoke(&self, action: &str, value: &str, arguments: &[&str]) -> Output {
        support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args([action, "--offline", "--manifest-path"])
                .arg(self.path().join("Cargo.toml"))
                .args(arguments)
                .env("HOST_RUNTIME_VALUE", value)
                .env("HOST_COUNTER_PATH", self.path().join("macro-count.txt"))
                .env("HOST_INPUT_PATH", self.path().join("macro-input.txt"))
                .env(
                    "HOST_EMPTY_COUNTER_PATH",
                    self.path().join("empty-count.txt"),
                ),
            self.path(),
            "host",
            Duration::from_secs(120),
        )
        .completed()
    }
    fn prepare(&self, value: &str, arguments: &[&str]) -> String {
        let output = self.invoke("prepare", value, arguments);
        support::oracle::success(&output, "prepare native hosts");
        String::from_utf8(output.stderr).unwrap()
    }
}

#[test]
fn script_compilation_reuses_inputs_but_execution_observes_changes() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','helper']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[build-dependencies]\nhelper={path='../helper'}\n");
    f.write("app/src/main.rs", "fn main(){}\n");
    f.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write(
        "helper/src/lib.rs",
        "mod child; pub fn value()->u32{child::value()}\n",
    );
    f.write("helper/src/child.rs", "pub fn value()->u32{11}\n");
    f.write("prefix.txt", "prefix");
    f.write("app/build.rs", r#"
fn main(){
    let runs=std::fs::read_to_string("runs.txt").unwrap_or_default();
    std::fs::write("runs.txt",format!("{runs}x")).unwrap();
    let text=format!("{} {} {}",include_str!("../prefix.txt"),helper::value(),std::env::var("HOST_RUNTIME_VALUE").unwrap());
    std::fs::write("observed.txt",text).unwrap();
}
"#);
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n");
    let first = f.prepare("first", &[]);
    assert!(!first.contains("fresh flux host"), "{first}");
    let second = f.prepare("second", &[]);
    assert!(second.contains("fresh flux host"), "{second}");
    assert_eq!(
        std::fs::read_to_string(f.path().join("app/runs.txt")).unwrap(),
        "xx"
    );
    assert_eq!(
        std::fs::read_to_string(f.path().join("app/observed.txt")).unwrap(),
        "prefix 11 second"
    );
    f.write("prefix.txt", "changed");
    let included = f.prepare("third", &[]);
    assert!(!included.contains("fresh flux host"), "{included}");
    assert_eq!(
        std::fs::read_to_string(f.path().join("app/observed.txt")).unwrap(),
        "changed 11 third"
    );
    f.write("helper/src/child.rs", "pub fn value()->u32{13}\n");
    let dependency = f.prepare("fourth", &[]);
    assert!(!dependency.contains("fresh flux host"), "{dependency}");
    assert_eq!(
        std::fs::read_to_string(f.path().join("app/observed.txt")).unwrap(),
        "changed 13 fourth"
    );
    let fresh = f.prepare("fifth", &[]);
    assert!(fresh.contains("fresh flux host"), "{fresh}");
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(f.path().join("target/flux/native-prepared-plan.json")).unwrap(),
    )
    .unwrap();
    let executable = report["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|output| output["artifact"].as_str())
        .find(|path| path.ends_with("/build-script"))
        .unwrap();
    std::fs::write(format!("{executable}.host-fresh.json"), "malformed record").unwrap();
    let malformed = f.prepare("sixth", &[]);
    assert!(!malformed.contains("fresh flux host"), "{malformed}");
    // Adding an alternative module candidate must invalidate the executable.
    f.write("helper/src/child/mod.rs", "pub fn value()->u32{17}\n");
    let ambiguous = f.invoke("prepare", "sixth", &[]);
    assert!(
        !ambiguous.status.success(),
        "ambiguous module must not reuse a host"
    );
}

#[test]
fn macro_host_reuse_still_expands_and_tracks_generated_root_inputs() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','macros']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='../macros'}\n");
    f.write(
        "app/src/main.rs",
        "fn main(){println!(\"{}\",macros::value!());}\n",
    );
    f.write(
        "macros/Cargo.toml",
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    let source = r#"
extern crate proc_macro;
mod helper;
#[proc_macro]
pub fn value(_:proc_macro::TokenStream)->proc_macro::TokenStream{
    let runtime=std::env::var("HOST_RUNTIME_VALUE").unwrap();
    format!("{}",helper::value()+runtime.parse::<u32>().unwrap()).parse().unwrap()
}
"#;
    f.write("macros/src/lib.rs", source);
    f.write("macros/src/helper.rs", "pub fn value()->u32{10}\n");
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['macros']\n[[package]]\nname='macros'\nversion='0.1.0'\n");
    for (value, expected, fresh) in [("1", "11\n", false), ("2", "12\n", true)] {
        let built = f.invoke("build", value, &[]);
        support::oracle::success(&built, "build with native macro");
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert_eq!(stderr.contains("fresh flux host macros"), fresh, "{stderr}");
        let output = Command::new(f.path().join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, "macro consumer");
        assert_eq!(output.stdout, expected.as_bytes());
    }
    f.write(
        "macros/src/lib.rs",
        &source.replace("helper::value()+runtime", "helper::value()+1+runtime"),
    );
    let root = f.prepare("2", &[]);
    assert!(!root.contains("fresh flux host macros"), "{root}");
    let cached = f.prepare("2", &[]);
    assert!(cached.contains("fresh flux host macros"), "{cached}");
    f.write("macros/src/helper.rs", "pub fn value()->u32{20}\n");
    let module = f.prepare("2", &[]);
    assert!(!module.contains("fresh flux host macros"), "{module}");
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(f.path().join("target/flux/native-prepared-plan.json")).unwrap(),
    )
    .unwrap();
    let executable = report["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|output| output["artifact"].as_str())
        .find(|path| path.ends_with("/host"))
        .unwrap();
    std::fs::write(executable, b"corrupt host").unwrap();
    let corrupt = f.prepare("2", &[]);
    assert!(!corrupt.contains("fresh flux host macros"), "{corrupt}");
}

#[test]
fn unchanged_macro_tokens_reuse_target_after_effects_and_diagnostics() {
    let f = Fixture::new();
    f.write("Cargo.toml", "[workspace]\nmembers=['app','macros','helper','leaf']\ndefault-members=['app']\nresolver='3'\n");
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='../macros'}\nhelper={path='../helper'}\n");
    f.write(
        "app/src/main.rs",
        "fn main(){println!(\"{} {}\",macros::value!(),helper::value());}\n",
    );
    f.write("helper/Cargo.toml", "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nleaf={path='../leaf'}\n");
    f.write(
        "helper/src/lib.rs",
        "pub fn value()->u32{leaf::value()+1}\n",
    );
    f.write(
        "leaf/Cargo.toml",
        "[package]\nname='leaf'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("leaf/src/lib.rs", "pub fn value()->u32{100}\n");
    f.write(
        "macros/Cargo.toml",
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    f.write(
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
#[proc_macro]
pub fn value(_:proc_macro::TokenStream)->proc_macro::TokenStream {
    let counter=std::env::var("HOST_COUNTER_PATH").unwrap();
    let previous=std::fs::read_to_string(&counter).unwrap_or_default();
    std::fs::write(&counter,format!("{previous}x")).unwrap();
    let runtime=std::env::var("HOST_RUNTIME_VALUE").unwrap();
    if runtime=="diagnostic" {
        return "compile_error!(\"host reuse probe diagnostic\")".parse().unwrap();
    }
    if runtime=="malformed" {
        return "1 +".parse().unwrap();
    }
    let input=std::fs::read_to_string(std::env::var("HOST_INPUT_PATH").unwrap()).unwrap();
    let value=input.parse::<u32>().unwrap()+runtime.parse::<u32>().unwrap();
    format!("{value}u32").parse().unwrap()
}
"#,
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['helper','macros']\n[[package]]\nname='helper'\nversion='0.1.0'\ndependencies=['leaf']\n[[package]]\nname='leaf'\nversion='0.1.0'\n[[package]]\nname='macros'\nversion='0.1.0'\n");
    f.write("macro-input.txt", "10");
    let mut counter = 0;
    for (step, value, expected, reused) in [
        ("initial", "1", "11 101\n", false),
        ("identical", "1", "11 101\n", true),
        ("environment", "2", "12 101\n", false),
        ("file", "2", "22 101\n", false),
        ("identical_again", "2", "22 101\n", true),
        ("transitive", "2", "22 201\n", false),
    ] {
        if step == "file" {
            f.write("macro-input.txt", "20");
        }
        if step == "transitive" {
            f.write("leaf/src/lib.rs", "pub fn value()->u32{200}\n");
        }
        let built = f.invoke("build", value, &[]);
        support::oracle::success(&built, step);
        let stderr = String::from_utf8_lossy(&built.stderr);
        assert_eq!(
            stderr.contains("Reused app after expansion"),
            reused,
            "{step}: {stderr}"
        );
        if step != "initial" {
            assert!(
                stderr.contains("fresh flux host macros"),
                "{step}: {stderr}"
            );
        }
        let current = std::fs::read_to_string(f.path().join("macro-count.txt"))
            .unwrap()
            .len();
        assert!(
            current > counter,
            "{step}: cached target skipped macro execution"
        );
        counter = current;
        let output = Command::new(f.path().join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, step);
        assert_eq!(output.stdout, expected.as_bytes(), "{step}");
    }
    for value in ["diagnostic", "malformed"] {
        let failed = f.invoke("build", value, &[]);
        assert!(
            !failed.status.success(),
            "{value}: reused an earlier successful target"
        );
        let stderr = String::from_utf8_lossy(&failed.stderr);
        assert!(
            !stderr.contains("Reused app after expansion"),
            "{value}: {stderr}"
        );
        if value == "diagnostic" {
            assert!(stderr.contains("host reuse probe diagnostic"), "{stderr}");
        }
        let current = std::fs::read_to_string(f.path().join("macro-count.txt"))
            .unwrap()
            .len();
        assert!(current > counter, "{value}: skipped macro execution");
        counter = current;
    }
}

#[test]
fn unused_macro_reexports_are_fresh_but_empty_expansions_still_run() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','adapter','macros']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write("app/Cargo.toml","[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nadapter={path='../adapter'}\n");
    f.write("adapter/Cargo.toml","[package]\nname='adapter'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='../macros'}\n");
    f.write(
        "macros/Cargo.toml",
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    f.write("Cargo.lock","version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['adapter']\n[[package]]\nname='adapter'\nversion='0.1.0'\ndependencies=['macros']\n[[package]]\nname='macros'\nversion='0.1.0'\n");
    f.write(
        "app/src/main.rs",
        "fn main(){println!(\"{}\",adapter::value());}",
    );
    f.write(
        "adapter/src/lib.rs",
        "pub use macros::{value,empty};pub fn value()->u32{1}",
    );
    f.write(
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
fn record(name:&str){
    let path=std::env::var(name).unwrap();
    let previous=std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(path,format!("{previous}x")).unwrap();
}
#[proc_macro]
pub fn value(_:proc_macro::TokenStream)->proc_macro::TokenStream{
    record("HOST_COUNTER_PATH");
    std::env::var("HOST_RUNTIME_VALUE").unwrap().parse().unwrap()
}
#[proc_macro]
pub fn empty(_:proc_macro::TokenStream)->proc_macro::TokenStream{
    record("HOST_EMPTY_COUNTER_PATH");
    proc_macro::TokenStream::new()
}
"#,
    );
    let success = |value: &str| {
        let output = f.invoke("build", value, &[]);
        support::oracle::success(&output, "macro reexport build");
        String::from_utf8(output.stderr).unwrap()
    };
    success("1");
    let unused = success("1");
    assert!(
        unused.contains("Fresh adapter") && unused.contains("Fresh app"),
        "{unused}"
    );
    assert!(!f.path().join("macro-count.txt").exists());
    f.write(
        "adapter/src/lib.rs",
        "pub use macros::{value,empty};pub fn value()->u32{macros::value!()}",
    );
    success("1");
    let before = std::fs::read(f.path().join("macro-count.txt"))
        .unwrap()
        .len();
    let same = success("1");
    assert!(
        same.contains("Reused adapter after expansion") && same.contains("Fresh app"),
        "{same}"
    );
    assert!(
        std::fs::read(f.path().join("macro-count.txt"))
            .unwrap()
            .len()
            > before
    );
    let changed = success("2");
    assert!(
        changed.contains("Compiling app") && !changed.contains("Fresh app"),
        "{changed}"
    );
    let output = Command::new(f.path().join("target/flux/debug/app"))
        .output()
        .unwrap();
    support::oracle::success(&output, "changed transitive expansion");
    assert_eq!(output.stdout, b"2\n");
    f.write(
        "app/src/main.rs",
        "adapter::empty!();fn main(){println!(\"{}\",adapter::value());}",
    );
    success("2");
    let before = std::fs::read(f.path().join("empty-count.txt"))
        .unwrap()
        .len();
    let empty = success("2");
    assert!(
        empty.contains("Reused app after expansion") && !empty.contains("Fresh app"),
        "{empty}"
    );
    assert!(
        std::fs::read(f.path().join("empty-count.txt"))
            .unwrap()
            .len()
            > before
    );
}
