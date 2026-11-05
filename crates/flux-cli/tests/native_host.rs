//! Native host tools retain features, macro dependencies and script output.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

struct Fixture {
    root: PathBuf,
    rustc: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../out/native-host-tests/{}",
            std::process::id()
        ));
        std::fs::create_dir_all(root.join("tools")).unwrap();
        let root = root.canonicalize().unwrap();
        let output = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .unwrap();
        assert!(output.status.success());
        let rustc = Path::new(String::from_utf8_lossy(&output.stdout).trim()).join("bin/rustc");
        for tool in ["cc", "clang", "ld", "ar", "as", "git", "xcrun", "bash"] {
            let found = Command::new("which").arg(tool).output().unwrap();
            if found.status.success() {
                std::os::unix::fs::symlink(
                    String::from_utf8(found.stdout).unwrap().trim(),
                    root.join("tools").join(tool),
                )
                .unwrap();
            }
        }
        let f = Self { root, rustc };
        f.write(
            "tools/cargo",
            "#!/bin/sh\necho called > \"$CARGO_TRAP\"\nexit 93\n",
        );
        f.write(
            "tools/rustc",
            "#!/bin/sh\necho called > \"$CARGO_TRAP\"\nexit 94\n",
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            f.root.join("tools/rustc"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::fs::set_permissions(
            f.root.join("tools/cargo"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        f
    }
    fn write(&self, path: &str, source: &str) {
        let path = self.root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    fn run(&self, action: &str, label: &str, args: &[&str]) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
        command
            .args([action, "--manifest-path"])
            .arg(self.root.join("Cargo.toml"))
            .args(["-p", "app", "--plan"])
            .arg(self.root.join(format!("{label}.json")))
            .args(args)
            .env("PATH", self.root.join("tools"))
            .env(
                "RUSTC",
                if args.contains(&"flux") {
                    self.root.join("tools/rustc")
                } else {
                    self.rustc.clone()
                },
            )
            .env("CARGO_HOME", self.root.join("cache"))
            .env("FLUX_BUILTINS_TRACE", "1")
            .env("CARGO_TRAP", self.root.join("cargo-was-called"))
            .env("CARGO_FEATURE_WRONG", "1")
            .env("CARGO_CFG_TARGET_OS", "wrong")
            .env("DEP_OTHER_WRONG", "wrong");
        let output =
            support::process::run(&mut command, &self.root, label, Duration::from_secs(120))
                .completed();
        if action == "prepare" {
            assert!(!String::from_utf8_lossy(&output.stderr).contains("flux builtins:"));
        }
        output
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
}

#[test]
fn direct_host_tools_feed_native_flux_builds() {
    let f = Fixture::new();
    f.write(
        "Cargo.toml",
        "[workspace]\nmembers=[\"app\",\"shared\",\"macros\",\"child\"]\nresolver=\"3\"\n",
    );
    f.write(
        "app/Cargo.toml",
        r#"
[package]
name="app"
version="0.1.0"
edition="2024"
[dependencies]
shared={path="../shared",features=["target"]}
alias={package="child",path="../child"}
macros={path="../macros",optional=true}
[build-dependencies]
macros={path="../macros"}
[features]
derive=["dep:macros"]
"#,
    );
    f.write(
        "app/build.rs",
        r#"
#[derive(macros::Answer)] struct Derived;
fn main() {
    assert_eq!(Derived::answer(),47);
    assert_eq!(std::env::var("DEP_CHILD_LIB_VERSION").unwrap(),"legacy");
    assert_eq!(std::env::var("DEP_CHILD_LIB_PATH").unwrap(),"modern=value");
    assert!(std::env::var_os("DEP_OTHER_WRONG").is_none());
    let out=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("value.rs"),"pub const GENERATED:usize=13;").unwrap();
    println!("cargo::rustc-cfg=generated");
    println!("cargo::rustc-check-cfg=cfg(generated)");
    println!("cargo::rustc-env=APP_VALUE=target-only");
}
"#,
    );
    f.write(
        "app/src/main.rs",
        r#"
include!(concat!(env!("OUT_DIR"),"/value.rs"));
#[cfg(not(generated))] compile_error!("missing generated cfg");
#[cfg(feature="derive")] #[derive(macros::Answer)] struct Derived;
fn main() {
    assert_eq!(env!("APP_VALUE"),"target-only");
    #[cfg(feature="derive")] assert_eq!(Derived::answer(),47);
    println!("{}",shared::value()+alias::value()+GENERATED+47);
}
"#,
    );
    f.write(
        "shared/Cargo.toml",
        r#"
[package]
name="shared"
version="0.1.0"
edition="2024"
[features]
host=[]
target=[]
"#,
    );
    f.write(
        "shared/build.rs",
        r#"
fn main() {
    assert!(std::env::var_os("CARGO_FEATURE_WRONG").is_none());
    assert_ne!(std::env::var("CARGO_CFG_TARGET_OS").unwrap(),"wrong");
    let value=if std::env::var_os("CARGO_FEATURE_HOST").is_some(){47}else{29};
    let out=std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    std::fs::write(out.join("value.rs"),format!("pub fn value()->usize{{{value}}}")).unwrap();
    println!("cargo:rustc-env=SHARED_ONLY=shared");
}
"#,
    );
    f.write(
        "shared/src/lib.rs",
        r#"
#[cfg(all(feature="host",feature="target"))] compile_error!("host and target features mixed");
include!(concat!(env!("OUT_DIR"),"/value.rs"));
pub const ENV:&str=env!("SHARED_ONLY");
"#,
    );
    f.write(
        "macros/Cargo.toml",
        r#"
[package]
name="macros"
version="0.1.0"
edition="2024"
[lib]
proc-macro=true
[dependencies]
helper={package="shared",path="../shared",features=["host"]}
"#,
    );
    f.write("macros/build.rs", "fn main(){println!(\"cargo::rustc-env=MACRO_MARKER=macro-only\");println!(\"cargo::rustc-cfg=macro_generated\"); if std::env::var(\"FLUX_COMPILER_PROBE\").is_ok(){let output=std::process::Command::new(std::env::var(\"RUSTC\").unwrap()).arg(\"--version\").output().unwrap();assert!(output.status.success());assert!(output.stdout.starts_with(b\"fluxc \"));}}\n");
    f.write(
        "macros/src/lib.rs",
        r#"
extern crate proc_macro;
#[cfg(not(macro_generated))] compile_error!("missing macro build cfg");
#[proc_macro_derive(Answer)]
pub fn answer(_:proc_macro::TokenStream)->proc_macro::TokenStream {
    assert_eq!(env!("MACRO_MARKER"),"macro-only");
    assert_eq!(helper::ENV,"shared");
    format!("impl Derived{{fn answer()->usize{{{}}}}}",helper::value()).parse().unwrap()
}
"#,
    );
    f.write(
        "child/Cargo.toml",
        "[package]\nname=\"child\"\nversion=\"0.1.0\"\nedition=\"2024\"\nlinks=\"child-lib\"\n",
    );
    f.write("child/build.rs", "fn main(){println!(\"cargo:VERSION=legacy\");println!(\"cargo::metadata=PATH=modern=value\");}\n");
    f.write("child/src/lib.rs", "pub fn value()->usize { 11 }\n");
    f.write(
        "Cargo.lock",
        r#"
version=4
[[package]]
name="app"
version="0.1.0"
dependencies=["child","shared","macros"]
[[package]]
name="child"
version="0.1.0"
[[package]]
name="shared"
version="0.1.0"
[[package]]
name="macros"
version="0.1.0"
dependencies=["shared"]
"#,
    );
    let lock = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    for (label, args) in [
        ("plain", vec![]),
        ("optimized", vec!["-O", "--features", "derive"]),
        (
            "flux-macros",
            vec![
                "-O",
                "--features",
                "derive",
                "--macro-host-compiler",
                "flux",
            ],
        ),
    ] {
        let output = f.run("build", label, &args);
        support::oracle::success(&output, "native host source build");
        let actual = support::process::run(
            &mut Command::new(f.root.join("target/flux/app")),
            &f.root,
            &format!("run-{label}"),
            Duration::from_secs(10),
        )
        .completed();
        support::oracle::success(&actual, "Flux native executable");
        assert_eq!(actual.stdout, b"100\n");
        let report: serde_json::Value =
            serde_json::from_slice(&std::fs::read(f.root.join(format!("{label}.json"))).unwrap())
                .unwrap();
        assert_eq!(report["kind"], "flux-native-prepared");
        let units = report["plan"]["units"].as_array().unwrap();
        let outputs = report["outputs"].as_array().unwrap();
        assert_eq!(units.len(), outputs.len());
        for (unit, output) in units.iter().zip(outputs) {
            if unit["domain"] == "target"
                || (label == "flux-macros" && unit["target"]["kind"] == "lib")
            {
                assert!(output["artifact"].is_null());
            } else {
                assert!(Path::new(output["artifact"].as_str().unwrap()).is_file());
            }
        }
        let shared = units
            .iter()
            .zip(outputs)
            .filter(|(u, _)| u["target"]["kind"] == "build-script")
            .filter(|(u, _)| {
                report["plan"]["packages"][u["package"].as_u64().unwrap() as usize]["name"]
                    == "shared"
            })
            .map(|(_, o)| o["script"]["out_dir"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(shared.len(), 2);
        assert_ne!(shared[0], shared[1]);
    }
    // A second preparation reuses fresh rustc artifacts at the same paths, and
    // rebuilds a library and its dependents when a source or generated file
    // it read changes.
    let report = |label: &str| -> serde_json::Value {
        serde_json::from_slice(&std::fs::read(f.root.join(format!("{label}.json"))).unwrap())
            .unwrap()
    };
    let executable = || std::fs::read(f.root.join("target/flux/app")).unwrap();
    let optimized = executable();
    let rerun = |label: &str, expected: &[u8]| -> String {
        let output = f.run("build", label, &["-O", "--features", "derive"]);
        support::oracle::success(&output, "repeated native host build");
        let actual = Command::new(f.root.join("target/flux/app"))
            .output()
            .unwrap();
        assert_eq!(actual.stdout, expected);
        String::from_utf8(output.stderr).unwrap()
    };
    let stderr = rerun("optimized-again", b"100\n");
    for unit in ["shared lib", "macros proc-macro"] {
        assert!(stderr.contains(&format!("fresh host {unit}")), "{stderr}");
    }
    assert!(!stderr.contains("rustc host"), "{stderr}");
    assert_eq!(
        report("optimized")["outputs"],
        report("optimized-again")["outputs"]
    );
    assert_eq!(executable(), optimized, "repeated builds must be identical");
    f.write(
        "shared/src/lib.rs",
        &std::fs::read_to_string(f.root.join("shared/src/lib.rs"))
            .unwrap()
            .replace("pub const ENV", "// edited\npub const ENV"),
    );
    let stderr = rerun("edited-source", b"100\n");
    for unit in ["shared lib", "macros proc-macro"] {
        assert!(stderr.contains(&format!("rustc host {unit}")), "{stderr}");
    }
    f.write(
        "shared/build.rs",
        &std::fs::read_to_string(f.root.join("shared/build.rs"))
            .unwrap()
            .replace("\"pub fn value", "\"// generated\\npub fn value"),
    );
    // Only the generated file changes, so only rustc's dep-info sees it.
    let stderr = rerun("edited-generated", b"100\n");
    assert!(stderr.contains("rustc host macros proc-macro"), "{stderr}");
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), lock);
    assert!(!f.root.join("cargo-was-called").exists());
    for (label, source, error) in [
        (
            "bad-directive",
            "fn main(){println!(\"cargo::rustc-link-arg=-bad\");}",
            "unsupported build-script directive",
        ),
        (
            "reported-error",
            "fn main(){println!(\"cargo::error=stop here\");}",
            "build script error: stop here",
        ),
        (
            "exit-error",
            "fn main(){eprintln!(\"script failure\");std::process::exit(7);}",
            "script failure",
        ),
    ] {
        f.write("app/build.rs", source);
        let output = f.run("prepare", label, &[]);
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(error),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!f.root.join(format!("{label}.json")).exists());
    }
}
