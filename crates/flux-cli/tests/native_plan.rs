//! Native planning must work with Cargo absent and must not execute build scripts.
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../out/native-plan-tests")
            .join(format!("{name}-{}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, name: &str, text: &str) {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
    fn run(&self, args: &[&str]) -> Output {
        let sysroot = Command::new("rustc")
            .args(["--print", "sysroot"])
            .output()
            .unwrap();
        assert!(sysroot.status.success());
        let rustc = Path::new(String::from_utf8_lossy(&sysroot.stdout).trim()).join("bin/rustc");
        let output = Command::new(env!("CARGO_BIN_EXE_flux"))
            .args(["cargo", "plan", "--manifest-path"])
            .arg(self.0.join("Cargo.toml"))
            .arg("--plan")
            .arg(self.0.join("plan.json"))
            .args(args)
            .env("PATH", self.0.join("tools"))
            .env("RUSTC", rustc)
            .env("CARGO_HOME", self.0.join("cache"))
            .env("FLUX_BUILTINS_TRACE", "1")
            .output()
            .unwrap();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("flux builtins:"));
        output
    }
    fn plan(&self, args: &[&str]) -> serde_json::Value {
        let result = self.run(args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        serde_json::from_slice(&std::fs::read(self.0.join("plan.json")).unwrap()).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
fn unit<'a>(
    plan: &'a serde_json::Value,
    name: &str,
    domain: &str,
    kind: &str,
) -> &'a serde_json::Value {
    plan["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| {
            u["target"]["name"] == name && u["domain"] == domain && u["target"]["kind"] == kind
        })
        .unwrap()
}
fn names(value: &serde_json::Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

#[test]
fn workspace_features_and_host_units_without_cargo() {
    let f = Fixture::new("features");
    f.write(
        "Cargo.toml",
        r#"
[workspace]
members=["app", "shared", "optional", "macros"]
resolver="3"
[workspace.package]
version="0.1.0"
edition="2024"
[workspace.dependencies]
shared={path="shared", default-features=false, features=["base"]}
"#,
    );
    f.write(
        "app/Cargo.toml",
        r#"
[package]
name="app"
version.workspace=true
edition.workspace=true
[dependencies]
shared={workspace=true, features=["target"]}
maybe={package="optional", path="../optional", optional=true, default-features=false}
macros={path="../macros", optional=true}
[build-dependencies]
shared={workspace=true, features=["host"]}
macros={path="../macros"}
[target.'cfg(any(target_os="none", all(unix, not(unix))))'.dependencies]
missing={path="../missing"}
[features]
default=["weak"]
weak=["maybe?/extra"]
enable=["dep:maybe"]
strong=["maybe/extra"]
derive=["dep:macros"]
"#,
    );
    f.write("app/src/main.rs", "fn main() {}\n");
    f.write("app/src/lib.rs", "\n");
    f.write(
        "app/build.rs",
        "compile_error!(\"planning must never compile or run this\");\n",
    );
    f.write(
        "shared/Cargo.toml",
        r#"
[package]
name="shared"
version.workspace=true
edition.workspace=true
[features]
base=[]
target=[]
host=[]
"#,
    );
    f.write("shared/src/lib.rs", "\n");
    f.write(
        "optional/Cargo.toml",
        r#"
[package]
name="optional"
version="0.1.0"
edition="2024"
[lib]
name="renamed_library"
[features]
extra=[]
"#,
    );
    f.write("optional/src/lib.rs", "\n");
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
shared={path="../shared", default-features=false, features=["host"]}
"#,
    );
    f.write("macros/src/lib.rs", "\n");
    f.write(
        "Cargo.lock",
        r#"
version=4
[[package]]
name="app"
version="0.1.0"
dependencies=["shared", "optional", "macros", "missing"]
[[package]]
name="shared"
version="0.1.0"
[[package]]
name="optional"
version="0.1.0"
[[package]]
name="macros"
version="0.1.0"
dependencies=["shared"]
"#,
    );
    let lock = std::fs::read(f.0.join("Cargo.lock")).unwrap();
    let plan = f.plan(&["-p", "app"]);
    assert_eq!(plan["kind"], "flux-native-offline");
    assert_eq!(
        names(&unit(&plan, "shared", "target", "lib")["features"]),
        ["base", "target"]
    );
    assert_eq!(
        names(&unit(&plan, "shared", "host", "lib")["features"]),
        ["base", "host"]
    );
    assert!(
        !plan["packages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["name"] == "optional")
    );
    assert!(
        unit(&plan, "app", "target", "bin")["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .all(|d| d[0] != "macros")
    );
    let script = unit(&plan, "build_script_build", "host", "build-script");
    assert_eq!(script["feature_domain"], "target");
    assert_eq!(script["dependencies"].as_array().unwrap().len(), 2);
    assert!(!f.0.join("target").exists());
    let bytes = std::fs::read(f.0.join("plan.json")).unwrap();
    f.plan(&["-p", "app"]);
    assert_eq!(std::fs::read(f.0.join("plan.json")).unwrap(), bytes);
    for features in ["enable", "strong"] {
        let plan = f.plan(&["-p", "app", "--features", features]);
        assert_eq!(
            names(&unit(&plan, "renamed_library", "target", "lib")["features"]),
            ["extra"]
        );
    }
    let plan = f.plan(&["-p", "app", "--features", "enable", "--no-default-features"]);
    assert!(names(&unit(&plan, "renamed_library", "target", "lib")["features"]).is_empty());
    let plan = f.plan(&["-p", "app", "--features", "derive"]);
    assert!(
        unit(&plan, "app", "target", "bin")["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d[0] == "macros")
    );
    let bad = f.run(&["-p", "app", "--features", "unknown"]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("unknown feature"));
    assert_eq!(std::fs::read(f.0.join("Cargo.lock")).unwrap(), lock);
}

#[test]
fn locked_registry_aliases_versions_and_missing_cache() {
    let f = Fixture::new("registry");
    f.write(
        "Cargo.toml",
        r#"
[workspace]
resolver="3"
[package]
name="app"
version="0.1.0"
edition="2024"
[dependencies]
old={package="sample", version="1"}
new={package="sample", version="2"}
"#,
    );
    f.write("src/main.rs", "fn main() {}\n");
    f.write(
        "Cargo.lock",
        r#"
version=4
[[package]]
name="app"
version="0.1.0"
dependencies=["sample 1.0.0", "sample 2.0.0"]
[[package]]
name="sample"
version="1.0.0"
source="registry+https://github.com/rust-lang/crates.io-index"
checksum="one"
[[package]]
name="sample"
version="2.0.0"
source="registry+https://github.com/rust-lang/crates.io-index"
checksum="two"
"#,
    );
    let missing = f.run(&[]);
    assert!(!missing.status.success());
    assert!(String::from_utf8_lossy(&missing.stderr).contains("offline source missing"));
    for (version, checksum) in [("1.0.0", "one"), ("2.0.0", "two")] {
        let root = format!("cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-{version}");
        f.write(
            &format!("{root}/Cargo.toml"),
            &format!("[package]\nname=\"sample\"\nversion=\"{version}\"\nedition=\"2024\"\n"),
        );
        f.write(&format!("{root}/src/lib.rs"), "\n");
        f.write(
            &format!("{root}/.cargo-checksum.json"),
            &format!(r#"{{"package":"{checksum}","files":{{}}}}"#),
        );
    }
    let plan = f.plan(&[]);
    assert_eq!(plan["packages"].as_array().unwrap().len(), 3);
    let deps = unit(&plan, "app", "target", "bin")["dependencies"]
        .as_array()
        .unwrap();
    assert_eq!(deps.len(), 2);
    assert_ne!(deps[0][1], deps[1][1]);
    f.write(
        "cache/registry/src/index.crates.io-1949cf8c6b5b557f/sample-1.0.0/.cargo-checksum.json",
        r#"{"package":"changed"}"#,
    );
    let bad = f.run(&[]);
    assert!(!bad.status.success());
    assert!(String::from_utf8_lossy(&bad.stderr).contains("cached checksum differs"));
}

#[test]
fn exact_git_checkout_and_stale_lock_rejection() {
    let f = Fixture::new("git");
    let checkout = "cache/git/checkouts/offline-test/checkout";
    f.write(
        &format!("{checkout}/Cargo.toml"),
        r#"
[workspace]
resolver="3"
[package]
name="cached-git"
version="0.1.0"
edition="2024"
"#,
    );
    f.write(&format!("{checkout}/src/lib.rs"), "\n");
    let path = f.0.join(checkout);
    for args in [
        vec!["init", "-q"],
        vec!["add", "."],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(&path)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let commit = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    let git = Command::new("which").arg("git").output().unwrap();
    assert!(git.status.success());
    std::fs::create_dir_all(f.0.join("tools")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        String::from_utf8(git.stdout).unwrap().trim(),
        f.0.join("tools/git"),
    )
    .unwrap();
    f.write(
        "Cargo.toml",
        &format!(
            r#"
[workspace]
resolver="3"
[package]
name="app"
version="0.1.0"
edition="2024"
[dependencies]
cached-git={{git="https://example.invalid/offline", rev="{commit}"}}
"#
        ),
    );
    f.write("src/main.rs", "fn main() {}\n");
    let lock = format!(
        r#"
version=4
[[package]]
name="app"
version="0.1.0"
dependencies=["cached-git"]
[[package]]
name="cached-git"
version="0.1.0"
source="git+https://example.invalid/offline?rev={commit}#{commit}"
"#
    );
    f.write("Cargo.lock", &lock);
    let plan = f.plan(&[]);
    assert_eq!(plan["packages"].as_array().unwrap().len(), 2);
    f.write(
        "Cargo.lock",
        &lock.replace(&format!("#{commit}"), &format!("#{}", "0".repeat(40))),
    );
    let output = f.run(&[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("offline source missing"));
}

#[test]
fn unsupported_overrides_and_stale_edges_fail() {
    let f = Fixture::new("failures");
    let manifest = r#"
[workspace]
resolver="3"
[package]
name="app"
version="0.1.0"
edition="2024"
[dependencies]
child={path="child"}
[features]
cli=[]
[[bin]]
name="app"
path="src/main.rs"
required-features=["cli"]
"#;
    f.write("Cargo.toml", manifest);
    f.write("src/main.rs", "fn main() {}\n");
    f.write(
        "child/Cargo.toml",
        "[package]\nname=\"child\"\nversion=\"0.1.0\"\nedition=\"2024\"\n",
    );
    f.write("child/src/lib.rs", "\n");
    let lock = "version=4\n[[package]]\nname=\"app\"\nversion=\"0.1.0\"\ndependencies=[\"child\"]\n[[package]]\nname=\"child\"\nversion=\"0.1.0\"\n";
    f.write("Cargo.lock", lock);
    let disabled = f.run(&[]);
    assert!(!disabled.status.success());
    assert!(String::from_utf8_lossy(&disabled.stderr).contains("requires disabled features"));
    f.plan(&["--features", "cli"]);
    f.write(
        "Cargo.lock",
        &lock.replace("dependencies=[\"child\"]", "dependencies=[]"),
    );
    let stale = f.run(&["--features", "cli"]);
    assert!(!stale.status.success());
    assert!(
        String::from_utf8_lossy(&stale.stderr).contains("absent from the parent's locked edges")
    );
    f.write("Cargo.lock", lock);
    f.write(
        ".cargo/config.toml",
        "[source.crates-io]\nreplace-with=\"vendored\"\n[source.vendored]\ndirectory=\"vendor\"\n",
    );
    let replaced = f.run(&["--features", "cli"]);
    assert!(!replaced.status.success());
    assert!(
        String::from_utf8_lossy(&replaced.stderr)
            .contains("cannot apply source or target overrides")
    );
    std::fs::remove_file(f.0.join(".cargo/config.toml")).unwrap();
    f.write(
        "Cargo.toml",
        &manifest.replace("resolver=\"3\"", "resolver=\"1\""),
    );
    let old = f.run(&["--features", "cli"]);
    assert!(!old.status.success());
    assert!(String::from_utf8_lossy(&old.stderr).contains("requires feature resolver 2 or 3"));
}
