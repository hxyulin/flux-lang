//! Cargo-produced host inputs feed Flux target source compilation.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64"),
))]
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
    support::process::run(command, root, label, Duration::from_secs(90)).completed()
}
#[test]
fn isolated_build_inputs_and_host_macros() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("../../out/cargo-host/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    write(
        &root,
        "Cargo.toml",
        r#"
[workspace]
members=["app","left","right","macros","optional"]
resolver="3"
"#,
    );
    for (name, value) in [("left", 17), ("right", 41), ("app", 7)] {
        let deps = if name == "app" {
            "[[bin]]\nname=\"app-tool\"\npath=\"src/main.rs\"\n[dependencies]\na={package=\"left\",path=\"../left\"}\nb={package=\"right\",path=\"../right\"}\nmacros={path=\"../macros\",optional=true}\nmaybe-lib={package=\"optional\",path=\"../optional\",optional=true}\n[build-dependencies]\nmacros={path=\"../macros\"}\n[features]\nmacros=[\"dep:macros\"]\ndefault=[\"alloc\"]\nalloc=[\"maybe-lib?/alloc\"]\noptional=[\"nested\"]\nnested=[\"dep:maybe-lib\"]\nstrong=[\"maybe-lib/alloc\"]\n"
        } else {
            ""
        };
        let version = if name == "app" {
            "0.1.0-rc.2+test"
        } else {
            "0.1.0"
        };
        write(
            &root,
            &format!("{name}/Cargo.toml"),
            &format!("[package]\nname=\"{name}\"\nversion=\"{version}\"\nedition=\"2024\"\n{deps}"),
        );
        write(
            &root,
            &format!("{name}/build.rs"),
            &format!(
                r#"
fn main() {{
    let out=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("value.rs"),"pub const GENERATED:usize={value};").unwrap();
    println!("cargo::rustc-env=VALUE_NAME={name}");
    println!("cargo::rustc-cfg=generated");
    println!("cargo::rustc-check-cfg=cfg(generated)");
}}
"#
            ),
        );
        write(
            &root,
            &format!("{name}/src/lib.rs"),
            &format!(
                r#"
include!(concat!(env!("OUT_DIR"),"/value.rs"));
#[cfg(not(generated))] compile_error!("build cfg was lost");
#[macro_export] macro_rules! caller_name {{ () => {{ env!("VALUE_NAME") }} }}
mod nested {{ pub const NAME:&str=env!("VALUE_NAME"); }}
pub fn value()->usize{{assert_eq!(nested::NAME,"{name}"); assert_eq!(env!("CARGO_PKG_NAME"),"{name}"); GENERATED}}
"#
            ),
        );
    }
    write(
        &root,
        "optional/Cargo.toml",
        "[package]\nname=\"optional\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\nname=\"different_lib_name\"\n[features]\nalloc=[]\n",
    );
    write(
        &root,
        "optional/src/lib.rs",
        "pub fn value()->usize { 99 }\n",
    );
    let script = root.join("app/build.rs");
    let source = std::fs::read_to_string(&script).unwrap().replace(
        "fn main() {",
        "fn main() { assert_eq!(Derived::answer(),42);",
    );
    std::fs::write(
        &script,
        format!("#[derive(macros::Answer)] struct Derived;\n{source}"),
    )
    .unwrap();
    write(
        &root,
        "macros/Cargo.toml",
        "[package]\nname=\"macros\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[lib]\nproc-macro=true\n",
    );
    write(
        &root,
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
#[proc_macro_derive(Answer)]
pub fn answer(_:proc_macro::TokenStream)->proc_macro::TokenStream{
    "impl Derived { fn answer()->usize { 42 } }".parse().unwrap()
}
"#,
    );
    write(
        &root,
        "app/src/main.rs",
        r#"
#[cfg(feature="macros")] #[derive(macros::Answer)] struct Derived;
include!(concat!(env!("OUT_DIR"),"/value.rs"));
fn main(){
    assert_eq!(a::caller_name!(),"app");
    assert_eq!(env!("CARGO_CRATE_NAME"),"app_tool");
    assert_eq!(env!("CARGO_BIN_NAME"),"app-tool");
    assert_eq!(env!("CARGO_PKG_VERSION"),"0.1.0-rc.2+test");
    assert_eq!(env!("CARGO_PKG_VERSION_PATCH"),"0");
    assert_eq!(env!("CARGO_PKG_VERSION_PRE"),"rc.2");
    assert_eq!(a::value()+b::value()+app::value()+GENERATED,72);
    #[cfg(feature="macros")] assert_eq!(Derived::answer(),42);
    #[cfg(any(feature="optional", feature="strong"))] assert_eq!(maybe_lib::value(),99);
    println!("cargo host inputs 72");
}
"#,
    );
    let manifest = root.join("Cargo.toml");
    let output = run(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest),
        &root,
        "lock",
    );
    support::oracle::success(&output, "lockfile");
    let plan = root.join("plan.json");
    for optimized in [false, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .args(["build", "--cargo", "--offline", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app", "--plan"])
            .arg(&plan);
        if optimized {
            command.arg("-O");
        }
        let output = run(&mut command, &root, &format!("build-{optimized}"));
        support::oracle::success(&output, "Flux Cargo source build");
        let prepared: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
        let sources = prepared["sources"].as_array().unwrap();
        assert!(sources.iter().all(|s| s["name"] != "different_lib_name"));
        assert!(
            sources
                .iter()
                .all(|s| s["proc_macros"].as_array().unwrap().is_empty()),
            "a build dependency must not enable an optional target proc macro"
        );
        let actual = run(
            &mut Command::new(root.join("target/flux/app-tool")),
            &root,
            &format!("run-{optimized}"),
        );
        support::oracle::success(&actual, "Flux native");
        assert_eq!(actual.stdout, b"cargo host inputs 72\n");
    }
    for feature in ["optional", "strong"] {
        let output = run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args(["build", "--cargo", "--offline", "--manifest-path"])
                .arg(&manifest)
                .args(["-p", "app", "--features", feature, "--plan"])
                .arg(&plan),
            &root,
            &format!("build-{feature}"),
        );
        support::oracle::success(&output, "enabled optional dependency");
        let prepared: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
        assert!(prepared["sources"].as_array().unwrap().iter().any(|s| {
            s["name"] == "different_lib_name"
                && s["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| f == "alloc")
        }));
        let actual = run(
            &mut Command::new(root.join("target/flux/app-tool")),
            &root,
            &format!("run-{feature}"),
        );
        support::oracle::success(&actual, "optional dependency execution");
        assert_eq!(actual.stdout, b"cargo host inputs 72\n");
        let reference = run(
            Command::new("cargo")
                .args(["run", "--offline", "--locked", "--quiet", "--manifest-path"])
                .arg(&manifest)
                .args(["-p", "app", "--features", feature]),
            &root,
            &format!("rustc-{feature}"),
        );
        support::oracle::success(&reference, "rustc optional dependency reference");
        assert_eq!(actual.stdout, reference.stdout);
    }
    let output = run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args(["prepare", "--cargo", "--offline", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app", "--features", "macros", "--plan"])
            .arg(&plan),
        &root,
        "prepare-macro",
    );
    support::oracle::success(&output, "host proc-macro preparation");
    let prepared: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
    assert_eq!(prepared["version"], 2);
    let scripts = prepared["build_scripts"].as_object().unwrap();
    assert_eq!(scripts.len(), 3);
    let dirs = scripts
        .values()
        .map(|s| s["out_dir"].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(dirs.len(), 3, "OUT_DIR must remain per crate");
    let macros = prepared["host_proc_macros"].as_array().unwrap();
    assert_eq!(macros.len(), 1);
    assert!(Path::new(macros[0]["artifact"].as_str().unwrap()).is_file());
    let output = run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args(["build", "--cargo", "--offline", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app", "--features", "macros", "--plan"])
            .arg(&plan),
        &root,
        "build-macro-target",
    );
    support::oracle::success(&output, "Flux target derive expansion");
    let actual = run(
        &mut Command::new(root.join("target/flux/app-tool")),
        &root,
        "run-macro-target",
    );
    support::oracle::success(&actual, "Flux target derive execution");
    assert_eq!(actual.stdout, b"cargo host inputs 72\n");
    let output = run(
        Command::new("cargo")
            .args(["run", "--offline", "--locked", "--quiet", "--manifest-path"])
            .arg(&manifest)
            .args(["-p", "app", "--features", "macros"]),
        &root,
        "rustc-macro-run",
    );
    support::oracle::success(&output, "rustc macro reference");
    assert_eq!(output.stdout, b"cargo host inputs 72\n");
}
