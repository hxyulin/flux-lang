//! Unchanged default-feature registry graphs, with native rustc comparisons.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn run(command: &mut Command, root: &Path, label: &str) -> Output {
    let output = support::process::run(command, root, label, Duration::from_secs(600)).completed();
    support::oracle::success(&output, label);
    output
}
fn compare_registry(name: &str, version: &str, source: &str) {
    compare_registry_dependencies(name, version, source, &[]);
}
fn compare_registry_dependencies(name: &str, version: &str, source: &str, extras: &[(&str, &str)]) {
    let mut dependencies: Vec<(&str, &str, &[&str])> = vec![(name, version, &[])];
    dependencies.extend(
        extras
            .iter()
            .map(|&(name, version)| (name, version, &[][..])),
    );
    compare_registry_feature_dependencies(name, source, &dependencies);
}
fn compare_registry_feature_dependencies(
    name: &str,
    source: &str,
    dependencies: &[(&str, &str, &[&str])],
) {
    let scratch = support::scratch::Scratch::new(
        Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../out/corpus-crates/{name}")),
    );
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    let dependencies: String = dependencies
        .iter()
        .map(|(name, version, features)| {
            format!("{name}={{version=\"={version}\",features={features:?}}}\n")
        })
        .collect();
    std::fs::write(root.join("Cargo.toml"), format!(
        "[workspace]\n[package]\nname=\"corpus-crate-consumer\"\nversion=\"0.0.0\"\nedition=\"2024\"\n[dependencies]\n{dependencies}"
    )).unwrap();
    std::fs::write(root.join("src/main.rs"), source).unwrap();
    let manifest = root.join("Cargo.toml");
    run(
        Command::new("cargo")
            .args(["generate-lockfile", "--offline", "--manifest-path"])
            .arg(&manifest),
        root,
        "lock",
    );
    let metadata = run(
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
    let mut originals = Vec::new();
    for package in metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|p| p["source"].is_string())
    {
        let upstream = Path::new(package["manifest_path"].as_str().unwrap())
            .parent()
            .unwrap();
        let sources = upstream.join("src");
        support::inventory::collect(
            if sources.is_dir() { &sources } else { upstream },
            &mut originals,
        );
        originals.push((
            upstream.join("Cargo.toml"),
            std::fs::read(upstream.join("Cargo.toml")).unwrap(),
        ));
    }
    let lock = std::fs::read(root.join("Cargo.lock")).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        run(
            Command::new("cargo")
                .args(["build", "--offline", "--locked", "--manifest-path"])
                .arg(&manifest),
            root,
            "rustc-build",
        );
        let expected = run(
            &mut Command::new(root.join("target/debug/corpus-crate-consumer")),
            root,
            "rustc-run",
        );
        for mode in ["debug", "fast", "release"] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_flux"));
            command
                .args([
                    "build",
                    "--offline",
                    "--macro-host-compiler",
                    "rustc",
                    "-j1",
                    "--manifest-path",
                ])
                .arg(&manifest);
            if mode == "fast" {
                command.arg("--fast-build");
            }
            if mode == "release" {
                command.arg("--release");
            }
            run(&mut command, root, &format!("flux-build-{mode}"));
            let actual = run(
                &mut Command::new(root.join(format!("target/flux/{mode}/corpus-crate-consumer"))),
                root,
                &format!("flux-run-{mode}"),
            );
            assert_eq!(
                actual.status.code(),
                expected.status.code(),
                "{name} {mode}"
            );
            assert_eq!(actual.stdout, expected.stdout, "{name} {mode}");
            assert_eq!(actual.stderr, expected.stderr, "{name} {mode}");
        }
    });
    assert!(
        support::inventory::changed(&originals).is_empty(),
        "registry sources changed"
    );
    assert_eq!(std::fs::read(root.join("Cargo.lock")).unwrap(), lock);
    if let Err(error) = outcome {
        std::panic::resume_unwind(error);
    }
}
#[test]
fn itertools_default_graph_matches_rustc() {
    compare_registry(
        "itertools",
        "0.15.0",
        include_str!("fixtures/corpus_crates/itertools.rs.txt"),
    );
}

#[test]
fn proc_macro2_default_graph_matches_rustc() {
    compare_registry(
        "proc-macro2",
        "1.0.107",
        include_str!("fixtures/corpus_crates/proc_macro2.rs.txt"),
    );
}

#[test]
fn quote_default_graph_matches_rustc() {
    compare_registry(
        "quote",
        "1.0.47",
        include_str!("fixtures/corpus_crates/quote.rs.txt"),
    );
}

#[test]
fn syn_default_graph_matches_rustc() {
    compare_registry(
        "syn",
        "3.0.10",
        include_str!("fixtures/corpus_crates/syn.rs.txt"),
    );
}

#[test]
fn url_default_graph_matches_rustc() {
    compare_registry(
        "url",
        "2.5.8",
        include_str!("fixtures/corpus_crates/url.rs.txt"),
    );
}

#[test]
fn tracing_subscriber_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "tracing-subscriber",
        "0.3.23",
        include_str!("fixtures/corpus_crates/tracing_subscriber.rs.txt"),
        &[("tracing", "0.1.44")],
    );
}

#[test]
fn num_traits_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "num-traits",
        "0.2.19",
        include_str!("fixtures/corpus_crates/num-traits.rs.txt"),
        &[("num-integer", "0.1.47")],
    );
}
#[test]
fn http_default_graph_matches_rustc() {
    compare_registry(
        "http",
        "1.5.0",
        include_str!("fixtures/corpus_crates/http.rs.txt"),
    );
}
#[test]
fn futures_default_graph_matches_rustc() {
    compare_registry(
        "futures",
        "0.3.34",
        include_str!("fixtures/corpus_crates/futures.rs.txt"),
    );
}
#[test]
fn base64_default_graph_matches_rustc() {
    compare_registry(
        "base64",
        "0.23.1",
        include_str!("fixtures/corpus_crates/base64.rs.txt"),
    );
}
#[test]
fn httparse_default_graph_matches_rustc() {
    compare_registry(
        "httparse",
        "1.10.1",
        include_str!("fixtures/corpus_crates/httparse.rs.txt"),
    );
}

#[test]
fn camino_default_graph_matches_rustc() {
    compare_registry(
        "camino",
        "1.2.6",
        include_str!("fixtures/corpus_crates/camino.rs.txt"),
    );
}

#[test]
fn unicode_segmentation_default_graph_matches_rustc() {
    compare_registry(
        "unicode-segmentation",
        "1.13.3",
        include_str!("fixtures/corpus_crates/unicode-segmentation.rs.txt"),
    );
}

#[test]
fn socket2_default_graph_matches_rustc() {
    compare_registry(
        "socket2",
        "0.6.5",
        include_str!("fixtures/corpus_crates/socket2.rs.txt"),
    );
}

#[test]
fn slotmap_default_graph_matches_rustc() {
    compare_registry(
        "slotmap",
        "1.1.1",
        include_str!("fixtures/corpus_crates/slotmap.rs.txt"),
    );
}

#[test]
fn time_default_graph_matches_rustc() {
    compare_registry(
        "time",
        "0.3.55",
        include_str!("fixtures/corpus_crates/time.rs.txt"),
    );
}

#[test]
fn num_bigint_default_graph_matches_rustc() {
    compare_registry(
        "num-bigint",
        "0.4.8",
        include_str!("fixtures/corpus_crates/num-bigint.rs.txt"),
    );
}
#[test]
fn num_rational_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "num-rational",
        "0.4.2",
        include_str!("fixtures/corpus_crates/num-rational.rs.txt"),
        &[("num-bigint", "0.4.8")],
    );
}

#[test]
fn num_complex_default_graph_matches_rustc() {
    compare_registry(
        "num-complex",
        "0.4.6",
        include_str!("fixtures/corpus_crates/num-complex.rs.txt"),
    );
}

#[test]
fn ahash_default_graph_matches_rustc() {
    compare_registry(
        "ahash",
        "0.8.12",
        include_str!("fixtures/corpus_crates/ahash.rs.txt"),
    );
}

#[test]
fn nix_default_graph_matches_rustc() {
    compare_registry(
        "nix",
        "0.31.3",
        include_str!("fixtures/corpus_crates/nix.rs.txt"),
    );
}

#[test]
fn ordered_float_default_graph_matches_rustc() {
    compare_registry(
        "ordered-float",
        "5.5.1",
        include_str!("fixtures/corpus_crates/ordered-float.rs.txt"),
    );
}

#[test]
fn bstr_default_graph_matches_rustc() {
    compare_registry(
        "bstr",
        "1.13.1",
        include_str!("fixtures/corpus_crates/bstr.rs.txt"),
    );
}

#[test]
fn regex_automata_default_graph_matches_rustc() {
    compare_registry(
        "regex-automata",
        "0.4.18",
        include_str!("fixtures/corpus_crates/regex-automata.rs.txt"),
    );
}

#[test]
fn ignore_default_graph_matches_rustc() {
    compare_registry(
        "ignore",
        "0.4.33",
        include_str!("fixtures/corpus_crates/ignore.rs.txt"),
    );
}

#[test]
fn rustix_default_graph_matches_rustc() {
    compare_registry(
        "rustix",
        "1.1.6",
        include_str!("fixtures/corpus_crates/rustix.rs.txt"),
    );
}

#[test]
fn tempfile_default_graph_matches_rustc() {
    compare_registry(
        "tempfile",
        "3.27.0",
        include_str!("fixtures/corpus_crates/tempfile.rs.txt"),
    );
}

#[test]
fn cargo_metadata_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "cargo_metadata",
        "0.23.1",
        include_str!("fixtures/corpus_crates/cargo_metadata.rs.txt"),
        &[("serde_json", "1.0.154")],
    );
}

#[test]
fn hyper_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "hyper",
        "1.12.0",
        include_str!("fixtures/corpus_crates/hyper.rs.txt"),
        &[("tokio", "1.54.1")],
    );
}

#[test]
fn env_logger_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "env_logger",
        "0.11.11",
        include_str!("fixtures/corpus_crates/env_logger.rs.txt"),
        &[("log", "0.4.35")],
    );
}

#[test]
fn bincode_default_graph_matches_rustc() {
    compare_registry(
        "bincode",
        "1.3.3",
        include_str!("fixtures/corpus_crates/bincode.rs.txt"),
    );
}

#[test]
fn flume_default_graph_matches_rustc() {
    compare_registry(
        "flume",
        "0.12.0",
        include_str!("fixtures/corpus_crates/flume.rs.txt"),
    );
}

#[test]
fn nom_default_graph_matches_rustc() {
    compare_registry(
        "nom",
        "8.0.0",
        include_str!("fixtures/corpus_crates/nom.rs.txt"),
    );
}

#[test]
fn toml_default_graph_matches_rustc() {
    compare_registry(
        "toml",
        "0.8.23",
        include_str!("fixtures/corpus_crates/toml.rs.txt"),
    );
}

#[test]
fn petgraph_default_graph_matches_rustc() {
    compare_registry(
        "petgraph",
        "0.8.3",
        include_str!("fixtures/corpus_crates/petgraph.rs.txt"),
    );
}

#[test]
fn tokio_util_default_graph_matches_rustc() {
    compare_registry_dependencies(
        "tokio-util",
        "0.7.20",
        include_str!("fixtures/corpus_crates/tokio_util.rs.txt"),
        &[("tokio", "1.54.1")],
    );
}

#[test]
fn chrono_default_graph_matches_rustc() {
    compare_registry(
        "chrono",
        "0.4.45",
        include_str!("fixtures/corpus_crates/chrono.rs.txt"),
    );
}

#[test]
fn uuid_v4_serde_features_match_rustc() {
    compare_registry_feature_dependencies(
        "uuid-v4-serde",
        include_str!("../../../tests/corpus/features/uuid.rs"),
        &[
            ("uuid", "1.29.0", &["v4", "serde"]),
            ("serde_json", "1.0.154", &[]),
        ],
    );
}

#[test]
fn logos_default_graph_matches_rustc() {
    compare_registry(
        "logos",
        "0.16.1",
        include_str!("../../../tests/corpus/consumers/logos.rs"),
    );
}

#[test]
fn logos_safe_source_graph_matches_rustc() {
    compare_registry_feature_dependencies(
        "logos",
        include_str!("../../../tests/corpus/consumers/logos.rs"),
        &[("logos", "0.16.1", &["forbid_unsafe"])],
    );
}
