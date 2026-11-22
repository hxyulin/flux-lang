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
fn two_build_scripts_reuse_their_shared_library() {
    let fixture = Fixture::new();
    fixture.write(
        "Cargo.toml",
        "[workspace]\nmembers=['first','second','helper']\nresolver='3'\n",
    );
    fixture.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    fixture.write(
        "helper/src/lib.rs",
        "pub fn value() -> u32 { env!(\"HELPER_VALUE\").parse().unwrap() }\n",
    );
    fixture.write(
        "helper/build.rs",
        r#"
fn main() {
    println!("cargo:rerun-if-changed=value.txt");
    let value = std::fs::read_to_string("value.txt").unwrap();
    println!("cargo:rustc-env=HELPER_VALUE={value}");
}
"#,
    );
    fixture.write("helper/value.txt", "11");
    for package in ["first", "second"] {
        fixture.write(
            &format!("{package}/Cargo.toml"),
            &format!("[package]\nname='{package}'\nversion='0.1.0'\nedition='2024'\n[build-dependencies]\nhelper={{path='../helper'}}\n"),
        );
        fixture.write(&format!("{package}/src/main.rs"), "fn main() {}\n");
        fixture.write(
            &format!("{package}/build.rs"),
            r#"
fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("value.txt"), helper::value().to_string()).unwrap();
}
"#,
        );
    }
    fixture.write(
        "Cargo.lock",
        "version=4\n[[package]]\nname='first'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n[[package]]\nname='second'\nversion='0.1.0'\ndependencies=['helper']\n",
    );

    let verify_outputs = |expected: &str| {
        let prepared: serde_json::Value = serde_json::from_slice(
            &std::fs::read(fixture.path().join("target/flux/native-prepared-plan.json")).unwrap(),
        )
        .unwrap();
        let values: Vec<_> = prepared["outputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|output| output["script"]["out_dir"].as_str())
            .map(|dir| Path::new(dir).join("value.txt"))
            .filter(|path| path.exists())
            .map(|path| std::fs::read_to_string(path).unwrap())
            .collect();
        assert_eq!(values, [expected, expected]);
    };

    let first = fixture.prepare("", &["--workspace", "-j2"]);
    assert_eq!(first.matches("Compiling helper\n").count(), 1, "{first}");
    assert_eq!(first.matches("Fresh helper\n").count(), 1, "{first}");
    verify_outputs("11");

    let artifacts = || {
        std::fs::read_dir(fixture.path().join("target/flux/native-host/flux-deps"))
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_string_lossy().starts_with("helper-")
                    && entry.path().extension().is_some_and(|ext| ext == "flib")
            })
            .map(|entry| entry.path())
            .collect::<Vec<_>>()
    };
    let first_artifacts = artifacts();
    assert_eq!(first_artifacts.len(), 1);

    fixture.write("helper/value.txt", "13");
    let edited = fixture.prepare("", &["--workspace", "-j2"]);
    assert_eq!(edited.matches("Compiling helper\n").count(), 1, "{edited}");
    assert_eq!(edited.matches("Fresh helper\n").count(), 1, "{edited}");
    verify_outputs("13");
    assert_eq!(artifacts(), first_artifacts);
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
        unused.contains("Fresh macros")
            && unused.contains("Fresh adapter")
            && unused.contains("Fresh app"),
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
    let same = f.invoke("build", "1", &["--timings"]);
    support::oracle::success(&same, "unchanged transitive expansion");
    let same = String::from_utf8_lossy(&same.stderr);
    assert!(
        same.contains("Reused adapter after expansion") && same.contains("Fresh app"),
        "{same}"
    );
    let adapter = same.split_once("Compiling adapter\n").unwrap().1;
    let adapter = adapter.split_once("Fresh app").unwrap().0;
    assert!(!adapter.contains("hir body lowering"), "{adapter}");
    let same_count = std::fs::read(f.path().join("macro-count.txt"))
        .unwrap()
        .len();
    assert_eq!(same_count, before + 1);
    let rebuilt = f.invoke("build", "2", &["--timings"]);
    support::oracle::success(&rebuilt, "changed transitive expansion");
    let changed = String::from_utf8_lossy(&rebuilt.stderr);
    assert!(
        changed.contains("Compiling app")
            && !changed.contains("Fresh app")
            && changed.contains("deferred dependency IR"),
        "{changed}"
    );
    assert_eq!(
        std::fs::read(f.path().join("macro-count.txt"))
            .unwrap()
            .len(),
        same_count + 1,
        "loading deferred IR must not repeat macro effects"
    );
    let output = Command::new(f.path().join("target/flux/debug/app"))
        .output()
        .unwrap();
    support::oracle::success(&output, "changed transitive expansion");
    assert_eq!(output.stdout, b"2\n");

    // Changing dependency metadata must disable the deferred path even when
    // the dependency's own semantic identity and source snapshot still match.
    let directory = f.path().join("target/flux/debug/deps");
    let wrapper = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("macros-")
                && path.extension().is_some_and(|ext| ext == "flib")
        })
        .unwrap();
    let saved = std::fs::read(&wrapper).unwrap();
    let mut archive = flux_cli::flib::Archive::decode(&saved).unwrap();
    let compiler = flux_hir::flib::digest(&std::fs::read(env!("CARGO_BIN_EXE_fluxc")).unwrap());
    let mut metadata = flux_cli::flib::Metadata::decode(&archive.metadata, compiler).unwrap();
    metadata
        .sections
        .iter_mut()
        .find(|part| part.tag == 8)
        .unwrap()
        .bytes
        .clear();
    archive.metadata = metadata.encode().unwrap();
    std::fs::write(&wrapper, archive.encode().unwrap()).unwrap();
    let malformed = f.invoke("build", "2", &["--timings"]);
    std::fs::write(&wrapper, saved).unwrap();
    let diagnostics = String::from_utf8_lossy(&malformed.stderr);
    assert!(
        !malformed.status.success()
            && diagnostics.contains("macros-")
            && diagnostics.contains("truncated metadata"),
        "{diagnostics}"
    );
    assert_eq!(
        std::fs::read(f.path().join("macro-count.txt"))
            .unwrap()
            .len(),
        same_count + 1
    );

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
    f.write(
        "macros/src/lib.rs",
        "extern crate proc_macro; use proc_macro::TokenStream;
         #[proc_macro] pub fn value(_: TokenStream) -> TokenStream { \"3\".parse().unwrap() }
         #[proc_macro] pub fn empty(_: TokenStream) -> TokenStream { TokenStream::new() }",
    );
    let rebuilt = success("2");
    assert!(
        rebuilt.contains("Compiling macros") && !rebuilt.contains("Fresh macros"),
        "{rebuilt}"
    );
    let output = Command::new(f.path().join("target/flux/debug/app"))
        .output()
        .unwrap();
    support::oracle::success(&output, "changed macro wrapper");
    assert_eq!(output.stdout, b"3\n");
}

#[test]
fn rustc_macro_consumer_reuses_only_expansions_without_tracked_inputs() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','macros']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write("app/Cargo.toml", "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nmacros={path='../macros'}\n");
    f.write(
        "macros/Cargo.toml",
        "[package]\nname='macros'\nversion='0.1.0'\nedition='2024'\n[lib]\nproc-macro=true\n",
    );
    f.write("Cargo.lock", "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['macros']\n[[package]]\nname='macros'\nversion='0.1.0'\n");
    f.write(
        "app/src/main.rs",
        "fn main() { println!(\"{}\", macros::value!()); }\n",
    );
    let source = r#"
#![feature(proc_macro_tracked_path)]
#[proc_macro]
pub fn value(_: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let path = std::env::current_dir().unwrap().join("value.txt");
    let value = std::fs::read_to_string(path).unwrap();
    format!("{value}").parse().unwrap()
}
"#;
    f.write("macros/src/lib.rs", source);
    f.write("value.txt", "10");
    let build = |expected: &str, fresh| {
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "rustc",
                    "--manifest-path",
                ])
                .arg(f.path().join("Cargo.toml"))
                .env("RUSTC_BOOTSTRAP", "macros"),
            f.path(),
            "rustc-macro-freshness",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "rustc macro consumer build");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(stderr.contains("Fresh app"), fresh, "{stderr}");
        let output = Command::new(f.path().join("target/flux/debug/app"))
            .output()
            .unwrap();
        support::oracle::success(&output, "rustc macro consumer");
        assert_eq!(output.stdout, expected.as_bytes());
    };

    build("10\n", false);
    build("10\n", true);
    f.write("value.txt", "20");
    build("10\n", true);

    let tracked = source.replace(
        "let value = std::fs::read_to_string(path).unwrap();",
        "proc_macro::tracked::path(&path); let value = std::fs::read_to_string(path).unwrap();",
    );
    f.write("macros/src/lib.rs", &tracked);
    build("20\n", false);
    build("20\n", false);
    f.write("value.txt", "30");
    build("30\n", false);
}

#[test]
fn build_scripts_overlap_when_the_normal_dependency_has_no_links_metadata() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=['app','helper']\ndefault-members=['app']\nresolver='3'\n",
    );
    f.write(
        "helper/Cargo.toml",
        "[package]\nname='helper'\nversion='0.1.0'\nedition='2024'\n",
    );
    f.write("helper/src/lib.rs", "pub fn answer() -> u32 { 42 }");
    f.write(
        "app/Cargo.toml",
        "[package]\nname='app'\nversion='0.1.0'\nedition='2024'\n[dependencies]\nhelper={path='../helper'}\n",
    );
    f.write(
        "app/src/main.rs",
        "fn main() { assert_eq!(helper::answer(), 42); }",
    );
    f.write(
        "Cargo.lock",
        "version=4\n[[package]]\nname='app'\nversion='0.1.0'\ndependencies=['helper']\n[[package]]\nname='helper'\nversion='0.1.0'\n",
    );
    let script = r#"
fn main() {
    use std::time::{Duration, Instant};

    let dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace = dir.parent().unwrap();
    std::fs::write(workspace.join("OWN.started"), "").unwrap();

    let start = Instant::now();
    while !workspace.join("OTHER.started").exists() {
        assert!(start.elapsed() < Duration::from_secs(20), "the peer script did not start");
        std::thread::sleep(Duration::from_millis(10));
    }
}
"#;
    for (own, other) in [("app", "helper"), ("helper", "app")] {
        f.write(
            &format!("{own}/build.rs"),
            &script
                .replace("OWN.started", &format!("{own}.started"))
                .replace("OTHER.started", &format!("{other}.started")),
        );
    }
    let output = f.invoke("build", "", &["-j2"]);
    support::oracle::success(&output, "independent build scripts");
    assert!(f.path().join("app.started").exists());
    assert!(f.path().join("helper.started").exists());
}
