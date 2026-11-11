//! Upstream Rayon panic-safety assertions run with Flux's own unwind runtime.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;

use flux_syntax::ast::{self, AstNode};
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

const TARGETS: &[(&str, &[&str])] = &[
    (
        "collect",
        &["collect_drop_on_unwind", "collect_drop_on_unwind_zst"],
    ),
    ("iter_panic", &["iter_panic_fuse", "iter_panic"]),
    ("sort-panic-safe", &["sort_panic_safe"]),
];

fn run(command: &mut Command, root: &Path, label: &str, seconds: u64) -> Output {
    support::process::run(command, root, label, Duration::from_secs(seconds)).completed()
}

fn build(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = run(command, root, label, 900);
    support::oracle::success(&output, label);
    output
}

fn check_case(binary: &Path, case: &str, root: &Path, label: &str) {
    let output = run(
        Command::new(binary).arg(case).env("RAYON_NUM_THREADS", "4"),
        root,
        label,
        300,
    );
    if case == "iter_panic" {
        assert_eq!(
            output.status.code(),
            Some(101),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stderr).contains("boom"));
    } else {
        support::oracle::success(&output, label);
    }
}

#[test]
fn upstream_collection_fuse_and_sort_panic_safety() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/rayon-unwinding"),
    );
    let root = scratch.path();
    let mut manifest = String::from(
        r#"
[workspace]
[package]
name = "rayon-unwind-consumer"
version = "0.1.0"
edition = "2021"
autobins = false
[dependencies]
rayon = "=1.12.0"
rand = "0.9"
[profile.dev]
panic = "unwind"
[profile.release]
panic = "unwind"
"#,
    );
    for &(target, _) in TARGETS {
        manifest.push_str(&format!(
            "\n[[bin]]\nname = {target:?}\npath = \"{target}.rs\"\n"
        ));
        std::fs::write(root.join(format!("{target}.rs")), "fn main() {}\n").unwrap();
    }
    let manifest_path = root.join("Cargo.toml");
    std::fs::write(&manifest_path, manifest).unwrap();
    build(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest_path),
        root,
        "lock",
    );
    let metadata = build(
        Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(&manifest_path),
        root,
        "metadata",
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let rayon = packages
        .iter()
        .find(|package| package["name"] == "rayon")
        .unwrap();
    assert_eq!(rayon["version"], "1.12.0");
    assert!(rayon["source"].as_str().unwrap().starts_with("registry+"));
    let upstream = Path::new(rayon["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut originals = Vec::new();
    for package in packages
        .iter()
        .filter(|package| package["source"].is_string())
    {
        let directory = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        support::inventory::collect(&directory.join("src"), &mut originals);
        let manifest = directory.join("Cargo.toml");
        originals.push((manifest.clone(), std::fs::read(manifest).unwrap()));
    }
    for &(target, cases) in TARGETS {
        let path = upstream.join("tests").join(format!("{target}.rs"));
        let bytes = std::fs::read(&path).unwrap();
        let source = std::str::from_utf8(&bytes).unwrap();
        // Replace libtest annotations with direct dispatch. Keep every upstream
        // function body, helper, assertion, workload size and dependency intact.
        let mut adapter = source
            .replace("#[test]\n", "")
            .replace("#[cfg_attr(not(panic = \"unwind\"), ignore)]\n", "")
            .replace(
                "#[cfg_attr(any(target_os = \"emscripten\", target_family = \"wasm\"), ignore)]\n",
                "",
            )
            .replace("#[should_panic(expected = \"boom\")]\n", "");
        assert!(!adapter.contains("#[test]"));
        adapter.push_str("\nfn main(){assert!(cfg!(panic=\"unwind\"));let case=std::env::args().nth(1).expect(\"test name\");match case.as_str(){\n");
        for case in cases {
            assert!(source.contains(&format!("fn {case}(")));
            adapter.push_str(&format!("{case:?}=>{case}(),\n"));
        }
        adapter.push_str("_=>panic!(\"unknown test\"),}}\n");
        std::fs::write(root.join(format!("{target}.rs")), adapter).unwrap();
        originals.push((path, bytes));
    }
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        build(
            Command::new("cargo")
                .args([
                    "build",
                    "--offline",
                    "--locked",
                    "--bins",
                    "--manifest-path",
                ])
                .arg(&manifest_path),
            root,
            "rust-build",
        );
        for &(target, cases) in TARGETS {
            for case in cases {
                check_case(
                    &root.join("target/debug").join(target),
                    case,
                    root,
                    &format!("rust-{case}"),
                );
            }
        }
        for profile in ["fast", "release"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "flux",
                    "-j1",
                    "--bins",
                    "--manifest-path",
                ])
                .arg(&manifest_path)
                .arg(if profile == "fast" {
                    "--fast-build"
                } else {
                    "--release"
                });
            build(&mut command, root, &format!("flux-build-{profile}"));
            for &(target, cases) in TARGETS {
                for case in cases {
                    check_case(
                        &root.join("target/flux").join(profile).join(target),
                        case,
                        root,
                        &format!("flux-{profile}-{case}"),
                    );
                }
            }
        }
    });
    assert!(
        support::inventory::changed(&originals).is_empty(),
        "upstream sources changed"
    );
    assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}

struct CoreCase {
    name: String,
    panics: bool,
    message: Option<String>,
}

fn core_case(binary: &Path, case: &CoreCase, root: &Path, label: &str) {
    let output = run(
        Command::new(binary)
            .arg(&case.name)
            .env("RAYON_NUM_THREADS", "4"),
        root,
        label,
        180,
    );
    if case.panics {
        assert_eq!(
            output.status.code(),
            Some(101),
            "{label}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        if let Some(message) = &case.message {
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(message),
                "{label}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    } else {
        support::oracle::success(&output, label);
    }
}

#[test]
fn upstream_core_scope_join_broadcast_and_handler_panics() {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/rayon-core-unwinding"),
    );
    let root = scratch.path();
    let manifest = root.join("Cargo.toml");
    std::fs::write(
        &manifest,
        r#"
[workspace]
[package]
name = "rayon-core-unwind-consumer"
version = "0.1.0"
edition = "2021"
[[bin]]
name = "core-panics"
path = "main.rs"
[dependencies]
rayon-core = "=1.13.0"
[profile.dev]
panic = "unwind"
[profile.release]
panic = "unwind"
"#,
    )
    .unwrap();
    std::fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
    build(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest),
        root,
        "lock",
    );
    let metadata = build(
        Command::new("cargo")
            .args([
                "metadata",
                "--offline",
                "--locked",
                "--format-version",
                "1",
                "--manifest-path",
            ])
            .arg(&manifest),
        root,
        "metadata",
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let core = packages
        .iter()
        .find(|package| package["name"] == "rayon-core")
        .unwrap();
    assert_eq!(core["version"], "1.13.0");
    assert!(core["source"].as_str().unwrap().starts_with("registry+"));
    let upstream = Path::new(core["manifest_path"].as_str().unwrap())
        .parent()
        .unwrap();
    let mut originals = Vec::new();
    for package in packages
        .iter()
        .filter(|package| package["source"].is_string())
    {
        let directory = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        support::inventory::collect(&directory.join("src"), &mut originals);
        let path = directory.join("Cargo.toml");
        originals.push((path.clone(), std::fs::read(path).unwrap()));
    }
    let unwind = std::fs::read(upstream.join("src/unwind.rs")).unwrap();
    std::fs::write(root.join("upstream_unwind.rs"), unwind).unwrap();
    let mut source = String::from(
        r#"
pub use rayon_core::{join, scope, scope_fifo, spawn, Scope, ScopeFifo, ThreadPoolBuilder};
use std::any::Any;
use std::sync::{Arc, Barrier, Mutex};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
#[path="upstream_unwind.rs"] mod unwind;
"#,
    );
    let mut cases = Vec::new();
    for (module, path) in [
        ("join", "src/join/test.rs"),
        ("scope", "src/scope/test.rs"),
        ("spawn", "src/spawn/test.rs"),
        ("broadcast", "src/broadcast/test.rs"),
        ("pool", "src/thread_pool/test.rs"),
        ("handlers", "src/test.rs"),
    ] {
        let text = std::fs::read_to_string(upstream.join(path)).unwrap();
        let parse = flux_syntax::parse_source_file(&text);
        assert!(parse.errors().is_empty(), "{path}: {:?}", parse.errors());
        let mut body = String::new();
        let mut names = Vec::new();
        for function in parse.root().children().filter_map(ast::Fn::cast) {
            let name = function.name().unwrap().syntax().text().to_owned();
            let helper =
                module == "pool" && matches!(name.as_str(), "count_handler" | "wait_for_counter");
            if !name.contains("panic") && !helper {
                continue;
            }
            assert!(function.unsafe_token().is_none() && function.async_token().is_none());
            let start = function.fn_token().unwrap().text_range().start() as usize;
            let end = function.syntax().text_range().end() as usize;
            // Extract the exact function, including its full signature and body.
            // Only the outer libtest attributes are replaced by dispatch metadata.
            body.push_str(&text[start..end]);
            body.push('\n');
            if helper {
                continue;
            }
            let attr_start = function.syntax().text_range().start() as usize;
            let attributes = &text[attr_start..start];
            let message = attributes
                .split_once("expected = \"")
                .map(|(_, rest)| rest.split('"').next().unwrap().to_owned());
            cases.push(CoreCase {
                name: format!("{module}::{name}"),
                panics: attributes.contains("should_panic"),
                message,
            });
            names.push(name);
        }
        assert!(!names.is_empty(), "{path}");
        source.push_str(&format!(
            "\nmod {module}_cases{{use super::*;\n{body}\npub fn run(case:&str){{match case{{\n"
        ));
        for name in names {
            source.push_str(&format!("{name:?}=>{name}(),\n"));
        }
        source.push_str("_=>panic!(\"unknown case\"),}}}\n");
    }
    assert_eq!(
        cases.len(),
        24,
        "update the pinned upstream panic inventory explicitly"
    );
    source.push_str("\nfn main(){assert!(cfg!(panic=\"unwind\"));let case=std::env::args().nth(1).expect(\"case\");let(module,name)=case.split_once(\"::\").unwrap();match module{\n");
    for module in ["join", "scope", "spawn", "broadcast", "pool", "handlers"] {
        source.push_str(&format!("{module:?}=>{module}_cases::run(name),\n"));
    }
    source.push_str("_=>panic!(\"unknown module\"),}}\n");
    std::fs::write(root.join("main.rs"), source).unwrap();
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        build(
            Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(&manifest),
            root,
            "rust-build",
        );
        for case in &cases {
            core_case(
                &root.join("target/debug/core-panics"),
                case,
                root,
                &format!("rust-{}", case.name.replace("::", "-")),
            );
        }
        for profile in ["fast", "release"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "flux",
                    "-j1",
                    "--manifest-path",
                ])
                .arg(&manifest)
                .arg(if profile == "fast" {
                    "--fast-build"
                } else {
                    "--release"
                });
            build(&mut command, root, &format!("flux-build-{profile}"));
            for case in &cases {
                core_case(
                    &root.join("target/flux").join(profile).join("core-panics"),
                    case,
                    root,
                    &format!("flux-{profile}-{}", case.name.replace("::", "-")),
                );
            }
        }
    });
    assert!(
        support::inventory::changed(&originals).is_empty(),
        "upstream sources changed"
    );
    assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
