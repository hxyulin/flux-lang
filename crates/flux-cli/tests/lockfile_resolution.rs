//! Native version selection is compared with Cargo using a local registry.
#![cfg(unix)]
mod support;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

struct Fixture {
    root: PathBuf,
    files: BTreeMap<String, String>,
}
impl Fixture {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../out/lockfile-resolution/{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut f = Self {
            root: root.canonicalize().unwrap(),
            files: BTreeMap::new(),
        };
        f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n");
        f.write("src/main.rs", "fn main(){}\n");
        put(
            &f.root,
            "tools/cargo",
            "#!/bin/sh\necho called > \"$CARGO_TRAP\"\nexit 93\n",
        );
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            f.root.join("tools/cargo"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::fs::create_dir_all(f.root.join("registry/index")).unwrap();
        f
    }
    fn write(&mut self, name: &str, text: &str) {
        put(&self.root, name, text);
        self.files.insert(name.into(), text.into());
    }
    fn package(&self, name: &str, version: &str, deps: Vec<Value>, features: Value, extra: Value) {
        let package = format!("{name}-{version}");
        let source = self.root.join("publish").join(&package);
        std::fs::create_dir_all(source.join("src")).unwrap();
        let mut manifest =
            format!("[package]\nname={name:?}\nversion={version:?}\nedition=\"2015\"\n");
        if let Some(version) = extra.get("rust_version").and_then(Value::as_str) {
            manifest.push_str(&format!("rust-version={version:?}\n"));
        }
        if let Some(links) = extra.get("links").and_then(Value::as_str) {
            manifest.push_str(&format!("links={links:?}\n"));
            put(&source, "build.rs", "fn main(){}\n");
        }
        for dep in &deps {
            let kind = match dep["kind"].as_str().unwrap_or("normal") {
                "normal" => "dependencies",
                "build" => "build-dependencies",
                "dev" => "dev-dependencies",
                _ => panic!("fixture dependency kind"),
            };
            if let Some(target) = dep["target"].as_str() {
                manifest.push_str(&format!(
                    "\n[target.{target:?}.{kind}.{}]\n",
                    dep["name"].as_str().unwrap()
                ));
            } else {
                manifest.push_str(&format!("\n[{kind}.{}]\n", dep["name"].as_str().unwrap()));
            }
            manifest.push_str(&format!(
                "version={}\noptional={}\ndefault-features={}\nfeatures={}\n",
                dep["req"], dep["optional"], dep["default_features"], dep["features"]
            ));
            if let Some(package) = dep["package"].as_str() {
                manifest.push_str(&format!("package={package:?}\n"));
            }
        }
        let mut merged = features.as_object().unwrap().clone();
        if let Some(features) = extra.get("features2").and_then(Value::as_object) {
            for (k, v) in features {
                merged.insert(k.clone(), v.clone());
            }
        }
        manifest.push_str("\n[features]\n");
        for (name, value) in merged {
            manifest.push_str(&format!("{name:?}={value}\n"));
        }
        put(&source, "Cargo.toml", &manifest);
        put(&source, "src/lib.rs", "pub fn value()->usize{7}\n");
        let archive = self.root.join("registry").join(format!("{package}.crate"));
        let output = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(source.parent().unwrap())
            .arg(&package)
            .output()
            .unwrap();
        success(&output, "fixture archive");
        let bytes = std::fs::read(&archive).unwrap();
        let checksum: String = Sha256::digest(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut record = json!({"name":name,"vers":version,"deps":deps,"cksum":checksum,"features":features,"yanked":false});
        for (name, value) in extra.as_object().unwrap() {
            record[name] = value.clone();
        }
        let path = shard(name);
        let file = self.root.join("registry/index").join(&path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        use std::io::Write;
        writeln!(
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&file)
                .unwrap(),
            "{record}"
        )
        .unwrap();
        // Cargo's offline resolver considers downloaded releases only.
        let cached = self
            .root
            .join("cache/registry/cache/index.crates.io-1949cf8c6b5b557f")
            .join(format!("{package}.crate"));
        std::fs::create_dir_all(cached.parent().unwrap()).unwrap();
        std::fs::copy(archive, cached).unwrap();
    }
    fn resolve(&self, label: &str, args: &[&str]) -> Output {
        let path = std::env::join_paths(
            std::iter::once(self.root.join("tools"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args(["generate-lockfile", "--manifest-path"])
                .arg(self.root.join("Cargo.toml"))
                .args([
                    "--index-url",
                    &format!("file://{}", self.root.join("registry/index").display()),
                ])
                .args(args)
                .env("CARGO_HOME", self.root.join("cache"))
                .env("PATH", path)
                .env("CARGO_TRAP", self.root.join("cargo-called"))
                .env("CARGO", self.root.join("tools/cargo"))
                .env("FLUX_BUILTINS_TRACE", "1"),
            &self.root,
            label,
            Duration::from_secs(60),
        )
        .completed();
        assert!(!String::from_utf8_lossy(&output.stderr).contains("flux builtins:"));
        assert!(!self.root.join("cargo-called").exists());
        output
    }
    fn compare_cargo(&self) {
        self.compare_cargo_mode(true);
    }
    fn compare_cargo_mode(&self, offline: bool) {
        let oracle = self.root.join("oracle");
        for (file, text) in &self.files {
            put(&oracle, file, text);
        }
        put(
            &oracle,
            ".cargo/config.toml",
            &(self
                .files
                .get(".cargo/config.toml")
                .cloned()
                .unwrap_or_default()
                + &format!(
                    "[source.crates-io]\nreplace-with=\"fixture\"\n[source.fixture]\nlocal-registry={:?}\n",
                    self.root.join("registry").to_str().unwrap()
                )),
        );
        let mut command = Command::new("cargo");
        command
            .args(["generate-lockfile"])
            .current_dir(&oracle)
            .env("CARGO_HOME", self.root.join("oracle-home"));
        if offline {
            command.arg("--offline");
        }
        let output = support::process::run(
            &mut command,
            &self.root,
            "cargo-oracle",
            Duration::from_secs(60),
        )
        .completed();
        success(&output, "Cargo version oracle");
        assert_eq!(
            graph(&self.root.join("Cargo.lock")),
            graph(&oracle.join("Cargo.lock"))
        );
        std::fs::copy(self.root.join("Cargo.lock"), oracle.join("Cargo.lock")).unwrap();
        let output = support::process::run(
            command.arg("--locked"),
            &self.root,
            "cargo-accepts-lock",
            Duration::from_secs(60),
        )
        .completed();
        success(&output, "Cargo accepts native lockfile");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.root).unwrap();
        }
    }
}
fn put(root: &Path, name: &str, text: &str) {
    let path = root.join(name);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}
fn success(output: &Output, label: &str) {
    support::oracle::success(output, label);
}
fn dep(name: &str, req: &str) -> Value {
    json!({"name":name,"req":req,"features":[],"default_features":true,"optional":false,"target":null,"kind":"normal","registry":null})
}
fn shard(name: &str) -> String {
    match name.len() {
        1 => format!("1/{name}"),
        2 => format!("2/{name}"),
        3 => format!("3/{}/{name}", &name[..1]),
        _ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
    }
}
fn graph(path: &Path) -> BTreeMap<String, (Option<String>, BTreeSet<String>)> {
    let table: toml::Table = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let packages = table["package"].as_array().unwrap();
    packages
        .iter()
        .map(|p| {
            let key = format!(
                "{} {} {}",
                p["name"].as_str().unwrap(),
                p["version"].as_str().unwrap(),
                p.get("source")
                    .and_then(toml::Value::as_str)
                    .unwrap_or("path")
            );
            let dependencies = p
                .get("dependencies")
                .and_then(toml::Value::as_array)
                .into_iter()
                .flatten()
                .map(|s| {
                    let text = s.as_str().unwrap();
                    let mut parts = text.splitn(3, ' ');
                    let name = parts.next().unwrap();
                    let version = parts.next();
                    let source = parts.next().map(|s| s.trim_matches(['(', ')']));
                    let matches: Vec<_> = packages
                        .iter()
                        .filter(|d| {
                            d["name"].as_str() == Some(name)
                                && version.is_none_or(|v| d["version"].as_str() == Some(v))
                                && source.is_none_or(|s| {
                                    d.get("source").and_then(toml::Value::as_str).is_some_and(
                                        |actual| {
                                            actual == s
                                                || actual
                                                    .rsplit_once('#')
                                                    .is_some_and(|(prefix, _)| prefix == s)
                                        },
                                    )
                                })
                        })
                        .collect();
                    assert_eq!(matches.len(), 1, "{text}");
                    let d = matches[0];
                    format!(
                        "{} {} {}",
                        name,
                        d["version"].as_str().unwrap(),
                        d.get("source")
                            .and_then(toml::Value::as_str)
                            .unwrap_or("path")
                    )
                })
                .collect();
            (
                key,
                (
                    p.get("checksum")
                        .and_then(toml::Value::as_str)
                        .map(str::to_owned),
                    dependencies,
                ),
            )
        })
        .collect()
}

#[test]
fn semver_backtracking_and_incompatible_versions_match_cargo() {
    let mut f = Fixture::new("backtrack");
    for version in ["0.1.5", "1.2.0", "1.4.0", "1.9.0", "2.0.0", "3.0.0-alpha.1"] {
        f.package("shared", version, vec![], json!({}), json!({}));
    }
    f.package(
        "adapter",
        "1.0.0",
        vec![dep("shared", "^1.2")],
        json!({}),
        json!({}),
    );
    f.package(
        "adapter",
        "1.1.0",
        vec![dep("shared", "^1.8")],
        json!({}),
        json!({}),
    );
    f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nadapter=\"1\"\nshared=\">=1.0,<1.5\"\nold={package=\"shared\",version=\"0.1\"}\nnew={package=\"shared\",version=\"2\"}\n");
    success(&f.resolve("resolve", &[]), "backtracking resolution");
    f.compare_cargo();
    let graph = graph(&f.root.join("Cargo.lock"));
    assert!(graph.keys().any(|p| p.starts_with("adapter 1.0.0 ")));
    assert!(!graph.keys().any(|p| p.starts_with("adapter 1.1.0 ")));
    let bytes = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    success(
        &f.resolve("offline", &["--offline", "--locked"]),
        "offline locked resolution",
    );
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), bytes);
}
#[test]
fn features_optional_dependencies_and_missing_feature_backtracking() {
    let mut f = Fixture::new("features");
    f.package(
        "never-enabled",
        "1.0.0",
        vec![],
        json!({"std":[]}),
        json!({}),
    );
    f.package("macros", "1.0.0", vec![], json!({}), json!({}));
    let mut optional = dep("never-enabled", "1");
    optional["optional"] = json!(true);
    let mut macros = dep("macros", "1");
    macros["optional"] = json!(true);
    f.package(
        "wrapper",
        "1.0.0",
        vec![optional, macros],
        json!({"default":["weak"],"weak":["never-enabled?/std"]}),
        json!({"v":2,"features2":{"derive":["dep:macros"]}}),
    );
    f.package("wrapper", "1.1.0", vec![], json!({}), json!({}));
    f.package("optional-root", "1.0.0", vec![], json!({}), json!({}));
    f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nwrapper={version=\"1\",features=[\"derive\"]}\noptional-root={version=\"1\",optional=true}\n");
    success(&f.resolve("resolve", &[]), "feature-aware resolution");
    f.compare_cargo();
    let graph = graph(&f.root.join("Cargo.lock"));
    assert!(graph.keys().any(|p| p.starts_with("wrapper 1.0.0 ")));
    assert!(graph.keys().any(|p| p.starts_with("optional-root ")));
    assert!(graph.keys().any(|p| p.starts_with("never-enabled ")));
}
#[test]
fn workspace_dev_and_platform_dependencies_are_locked() {
    let mut f = Fixture::new("workspace");
    for name in ["normal", "dev", "platform", "helper-opt"] {
        f.package(name, "1.0.0", vec![], json!({}), json!({}));
    }
    f.write("Cargo.toml","[workspace]\nmembers=[\"app\"]\nresolver=\"3\"\n[workspace.package]\nversion=\"0.1.0\"\nedition=\"2024\"\n[workspace.dependencies]\nnormal=\"1\"\n");
    f.write("app/Cargo.toml","[package]\nname=\"app\"\nversion.workspace=true\nedition.workspace=true\n[dependencies]\nnormal.workspace=true\nhelper={path=\"../helper\"}\n[dev-dependencies]\ndev=\"1\"\n[target.'cfg(target_os=\"none\")'.dependencies]\nplatform=\"1\"\n");
    f.write("app/src/main.rs", "fn main(){}\n");
    f.write("helper/Cargo.toml","[package]\nname=\"helper\"\nversion.workspace=true\nedition.workspace=true\n[dependencies]\nhelper-opt={version=\"1\",optional=true}\n");
    f.write("helper/src/lib.rs", "");
    success(&f.resolve("resolve", &[]), "workspace lockfile");
    f.compare_cargo();
    assert_eq!(graph(&f.root.join("Cargo.lock")).len(), 6);
}
#[test]
fn yanked_rust_versions_and_links_conflicts_match_cargo() {
    let mut f = Fixture::new("policies");
    f.package(
        "policy",
        "1.0.0",
        vec![],
        json!({}),
        json!({"rust_version":"1.40"}),
    );
    f.package(
        "policy",
        "1.1.0",
        vec![],
        json!({}),
        json!({"rust_version":"1.80"}),
    );
    f.package("policy", "1.2.0", vec![], json!({}), json!({"yanked":true}));
    f.package("left", "1.0.0", vec![], json!({}), json!({}));
    f.package(
        "left",
        "2.0.0",
        vec![],
        json!({}),
        json!({"links":"native"}),
    );
    f.package(
        "right",
        "1.0.0",
        vec![],
        json!({}),
        json!({"links":"native"}),
    );
    f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2021\"\nrust-version=\"1.60\"\n[dependencies]\npolicy=\"1\"\nleft=\">=1,<3\"\nright=\"1\"\n");
    success(&f.resolve("resolve", &[]), "release policies");
    f.compare_cargo();
    let resolved = graph(&f.root.join("Cargo.lock"));
    assert!(resolved.keys().any(|p| p.starts_with("policy 1.0.0 ")));
    assert!(resolved.keys().any(|p| p.starts_with("left 1.0.0 ")));
    f.write(
        ".cargo/config.toml",
        "[resolver]\nincompatible-rust-versions=\"allow\"\n",
    );
    success(
        &f.resolve("allow-newer-rust", &[]),
        "configured Rust-version policy",
    );
    f.compare_cargo();
    assert!(
        graph(&f.root.join("Cargo.lock"))
            .keys()
            .any(|p| p.starts_with("policy 1.1.0 "))
    );
}
#[test]
fn failures_and_locked_updates_preserve_existing_lockfile() {
    let mut f = Fixture::new("atomic");
    f.package("sample", "1.0.0", vec![], json!({}), json!({}));
    let manifest = "[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nsample=\"1\"\n";
    f.write("Cargo.toml", manifest);
    success(&f.resolve("initial", &[]), "initial lockfile");
    let original = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    f.package("sample", "1.1.0", vec![], json!({}), json!({}));
    let changed = f.resolve("locked-update", &["--locked"]);
    assert!(!changed.status.success());
    assert!(String::from_utf8_lossy(&changed.stderr).contains("would change Cargo.lock"));
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), original);
    f.write(
        "Cargo.toml",
        &manifest.replace("sample=\"1\"", "sample=\"9\""),
    );
    let impossible = f.resolve("unsatisfiable", &[]);
    assert!(!impossible.status.success());
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), original);
    f.write("Cargo.toml", manifest);
    success(&f.resolve("update", &[]), "update to latest");
    assert!(
        graph(&f.root.join("Cargo.lock"))
            .keys()
            .any(|p| p.starts_with("sample 1.1.0 "))
    );
    let latest = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    let index = f.root.join("registry/index").join(shard("sample"));
    let records = std::fs::read_to_string(&index)
        .unwrap()
        .lines()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).unwrap();
            if value["vers"] == "1.1.0" {
                value["cksum"] = json!("0".repeat(64));
            }
            value.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(index, records + "\n").unwrap();
    let output = f.resolve("changed-checksum", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("registry checksum changed"));
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), latest);
}

#[test]
fn git_branches_tags_and_offline_refs_match_cargo() {
    let mut f = Fixture::new("git");
    f.write(
        "origin/Cargo.toml",
        "[workspace]\n[package]\nname=\"helper\"\nversion=\"1.0.0\"\nedition=\"2024\"\n",
    );
    f.write("origin/src/lib.rs", "pub fn value()->usize{7}\n");
    let origin = f.root.join("origin");
    for args in [
        vec!["init", "-q"],
        vec!["symbolic-ref", "HEAD", "refs/heads/work/branch"],
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
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "tag",
            "-a",
            "test/tag",
            "-m",
            "tag",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&origin)
            .args(args)
            .output()
            .unwrap();
        success(&output, "git fixture");
    }
    let url = format!("file://{}", origin.display());
    f.write("Cargo.toml",&format!("[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nhelper={{git={url:?},branch=\"work/branch\"}}\ntagged={{package=\"helper\",git={url:?},tag=\"test/tag\"}}\n"));
    success(&f.resolve("resolve", &[]), "resolve git refs");
    f.compare_cargo_mode(false);
    let bytes = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    assert!(String::from_utf8_lossy(&bytes).contains("branch=work%2Fbranch"));
    success(
        &f.resolve("offline", &["--offline", "--locked"]),
        "resolve cached refs offline",
    );
    let output = support::process::run(
        Command::new(env!("CARGO_BIN_EXE_flux"))
            .args(["plan", "--manifest-path"])
            .arg(f.root.join("Cargo.toml"))
            .arg("--plan")
            .arg(f.root.join("plan.json"))
            .env("CARGO_HOME", f.root.join("cache")),
        &f.root,
        "plan",
        Duration::from_secs(30),
    )
    .completed();
    success(&output, "native planner accepts encoded git refs");
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), bytes);
}
#[test]
fn resolve_fetch_and_build_without_existing_lockfile_or_cargo() {
    let mut f = Fixture::new("end-to-end");
    f.package("sample", "1.0.0", vec![], json!({}), json!({}));
    f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nsample=\"1\"\n");
    f.write(
        "src/main.rs",
        "fn main(){println!(\"{}\",sample::value());}\n",
    );
    std::fs::remove_dir_all(f.root.join("cache")).unwrap();
    assert!(!f.root.join("Cargo.lock").exists());
    success(&f.resolve("resolve", &[]), "create a fresh lockfile");
    let mirror = f.root.join("downloads/sample/1.0.0");
    std::fs::create_dir_all(&mirror).unwrap();
    std::fs::copy(
        f.root.join("registry/sample-1.0.0.crate"),
        mirror.join("download"),
    )
    .unwrap();
    let path = std::env::join_paths(
        std::iter::once(f.root.join("tools"))
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    let mut fetch = Command::new(env!("CARGO_BIN_EXE_flux"));
    fetch
        .args(["fetch", "--locked", "--manifest-path"])
        .arg(f.root.join("Cargo.toml"))
        .args([
            "--download-base",
            &format!("file://{}", f.root.join("downloads").display()),
        ])
        .env("CARGO_HOME", f.root.join("cache"))
        .env("PATH", &path)
        .env("CARGO_TRAP", f.root.join("cargo-called"));
    let output =
        support::process::run(&mut fetch, &f.root, "fetch", Duration::from_secs(30)).completed();
    success(&output, "fetch generated lockfile");
    if cfg!(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )) {
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args(["build", "--offline", "--manifest-path"])
                .arg(f.root.join("Cargo.toml"))
                .env("CARGO_HOME", f.root.join("cache"))
                .env("PATH", path)
                .env("CARGO_TRAP", f.root.join("cargo-called")),
            &f.root,
            "build",
            Duration::from_secs(90),
        )
        .completed();
        success(&output, "build resolved graph");
        let output = Command::new(f.root.join("target/flux/debug/app"))
            .output()
            .unwrap();
        success(&output, "run resolved graph");
        assert_eq!(output.stdout, b"7\n");
    }
    assert!(!f.root.join("cargo-called").exists());
}
#[test]
fn cycle_conflicts_backtrack_and_unsatisfiable_cycles_preserve_lock() {
    let mut f = Fixture::new("cycles");
    f.package("adapter", "1.0.0", vec![], json!({}), json!({}));
    f.package(
        "adapter",
        "1.1.0",
        vec![dep("loopback", "1")],
        json!({}),
        json!({}),
    );
    f.package(
        "loopback",
        "1.0.0",
        vec![dep("adapter", "=1.1.0")],
        json!({}),
        json!({}),
    );
    let manifest = "[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nadapter=\"1\"\n";
    f.write("Cargo.toml", manifest);
    success(&f.resolve("acyclic", &[]), "backtrack from cyclic release");
    assert!(
        graph(&f.root.join("Cargo.lock"))
            .keys()
            .any(|p| p.starts_with("adapter 1.0.0 "))
    );
    let original = std::fs::read(f.root.join("Cargo.lock")).unwrap();
    f.write(
        "Cargo.toml",
        &manifest.replace("adapter=\"1\"", "adapter=\"=1.1.0\""),
    );
    let output = f.resolve("cycle", &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("dependency cycle"));
    assert_eq!(std::fs::read(f.root.join("Cargo.lock")).unwrap(), original);
}

#[test]
fn edition_2024_workspace_default_features_can_be_disabled() {
    let mut f = Fixture::new("workspace-defaults");
    f.package("leaf", "1.0.0", vec![], json!({}), json!({}));
    let mut leaf = dep("leaf", "1");
    leaf["optional"] = json!(true);
    f.package(
        "featureful",
        "1.0.0",
        vec![leaf],
        json!({"default":["dep:leaf"]}),
        json!({"v":2}),
    );
    f.write("Cargo.toml","[workspace]\nmembers=[\"app\"]\nresolver=\"3\"\n[workspace.dependencies]\nfeatureful={version=\"1\",default-features=true}\n");
    f.write("app/Cargo.toml","[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nfeatureful={workspace=true,default-features=false}\n");
    f.write("app/src/main.rs", "fn main(){}\n");
    success(
        &f.resolve("resolve", &[]),
        "workspace default feature override",
    );
    f.compare_cargo();
    assert!(
        !graph(&f.root.join("Cargo.lock"))
            .keys()
            .any(|p| p.starts_with("leaf "))
    );
}

#[test]
fn regenerating_a_lockfile_rechecks_yanked_releases() {
    let mut f = Fixture::new("yanked-update");
    for version in ["1.0.0", "1.1.0"] {
        f.package("sample", version, vec![], json!({}), json!({}));
    }
    f.write("Cargo.toml","[workspace]\nresolver=\"3\"\n[package]\nname=\"app\"\nversion=\"0.1.0\"\nedition=\"2024\"\n[dependencies]\nsample=\"1\"\n");
    success(&f.resolve("initial", &[]), "initial release");
    f.compare_cargo();
    let index = f.root.join("registry/index").join(shard("sample"));
    let records = std::fs::read_to_string(&index)
        .unwrap()
        .lines()
        .map(|line| {
            let mut value: Value = serde_json::from_str(line).unwrap();
            if value["vers"] == "1.1.0" {
                value["yanked"] = json!(true);
            }
            value.to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    std::fs::write(index, records + "\n").unwrap();
    success(&f.resolve("after-yank", &[]), "regenerate after yank");
    f.compare_cargo();
}
