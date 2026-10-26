//! Underscore metavariables and unchanged cfg-if source use rustc as the oracle.
mod support;
use std::path::Path;
use std::process::{Command, Output};
use std::time::Duration;

fn bounded(command: &mut Command, directory: &Path, label: &str) -> Output {
    support::process::run(command, directory, label, Duration::from_secs(30)).completed()
}

#[test]
fn underscore_metavariable_names_match_rustc() {
    use flux_hir::SourceDb;
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let root = std::env::temp_dir().join(format!("flux-macro-underscore-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for (edition, name) in [
        (Edition::E2015, "2015"),
        (Edition::E2018, "2018"),
        (Edition::E2021, "2021"),
        (Edition::E2024, "2024"),
    ] {
        for (case, source, accepts) in [
            (
                "match",
                "macro_rules!take{($_:tt)=>{7u32};}fn main(){assert_eq!(take!(hello),7);}",
                true,
            ),
            (
                "reference",
                "macro_rules!take{($_:ident)=>{fn $_()->u32{7}};}take!(answer);fn main(){assert_eq!(answer(),7);}",
                true,
            ),
            (
                "nested",
                "macro_rules!take{($(($($_:tt)*)),*)=>{7};}fn main(){assert_eq!(take!((a b),(c)),7);}",
                true,
            ),
            (
                "transcribed_repeat",
                "macro_rules!take{($($_:expr),*)=>{0$(+$_)*};}fn main(){assert_eq!(take!(1,2,3),6);}",
                true,
            ),
            (
                "duplicate",
                "macro_rules!take{($_:tt $_:tt)=>{7};}fn main(){take!(a b);}",
                false,
            ),
            (
                "ident_input",
                "macro_rules!take{($name:ident)=>{7};}fn main(){take!(_);}",
                false,
            ),
            (
                "invalid_specifier",
                "macro_rules!take{($name:_)=>{7};}fn main(){}",
                false,
            ),
            (
                "repetition_depth",
                "macro_rules!take{($($_:tt)*)=>{$_};}fn main(){take!(a);}",
                false,
            ),
        ] {
            let path = root.join(format!("{case}-{name}.rs"));
            std::fs::write(&path, source).unwrap();
            let output = bounded(
                Command::new("rustc")
                    .arg(format!("--edition={name}"))
                    .args(["--emit=metadata", "-o"])
                    .arg(root.join(format!("{case}-{name}.rmeta")))
                    .arg(&path),
                &root,
                &format!("rustc-{case}-{name}"),
            );
            assert_eq!(
                output.status.success(),
                accepts,
                "rustc {case}/{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut db = SourceDb::new();
            let file = db.load_with_edition(&path, edition).unwrap();
            let hir = flux_hir::lower(&mut db, file, Symbol::intern("main"));
            let mut errors = db.syntax_errors();
            errors.extend(hir.diagnostics.iter().cloned());
            if !errors.is_empty() {
                assert!(!accepts, "{case}/{name}: {errors:?}");
                continue;
            }
            let typed = flux_mir::check_file(&hir);
            if !typed.diagnostics.is_empty() {
                assert!(!accepts, "{case}/{name}: {:?}", typed.diagnostics);
                continue;
            }
            let mir = flux_mir::build_file(&hir, &typed);
            assert_eq!(
                mir.diagnostics.is_empty(),
                accepts,
                "{case}/{name}: {:?}",
                mir.diagnostics
            );
        }
    }
}

#[test]
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn original_cfg_if_source_matches_rustc_off_and_on() {
    use flux_hir::{SourceCrate, SourceDb};
    use flux_intern::Symbol;
    use flux_syntax::Edition;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let root = repo.join(format!("out/cfg-if-readiness/{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let output = Command::new("cargo")
        .current_dir(&repo)
        .args(["metadata", "--offline", "--locked", "--format-version", "1"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let package = metadata["packages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "cfg-if" && p["version"] == "1.0.5")
        .unwrap();
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|n| n["id"] == package["id"])
        .unwrap();
    assert_eq!(node["features"], serde_json::json!([]));
    assert!(node["deps"].as_array().unwrap().is_empty());
    assert_eq!(package["edition"], "2018");
    let manifest = Path::new(package["manifest_path"].as_str().unwrap());
    let source = manifest.parent().unwrap().join("src/lib.rs");
    let originals = [
        (manifest.to_path_buf(), std::fs::read(manifest).unwrap()),
        (source.clone(), std::fs::read(&source).unwrap()),
    ];
    let consumer = root.join("consumer.rs");
    std::fs::write(&consumer, include_str!("fixtures/cfg_if/consumer.rs.txt")).unwrap();
    let library = root.join("libcfg_if.rlib");
    let output = bounded(
        Command::new("rustc")
            .args([
                "--edition=2018",
                "--crate-name=cfg_if",
                "--crate-type=rlib",
                "-Cpanic=abort",
                "-o",
            ])
            .arg(&library)
            .arg(&source),
        &root,
        "compile-rustc-library",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reference = root.join("reference");
    let output = bounded(
        Command::new("rustc")
            .args(["--edition=2024", "-Cpanic=abort", "--extern"])
            .arg(format!("cfg_if={}", library.display()))
            .arg(&consumer)
            .arg("-o")
            .arg(&reference),
        &root,
        "compile-rustc-consumer",
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected = bounded(&mut Command::new(reference), &root, "run-reference");
    assert!(
        expected.status.success(),
        "{}",
        String::from_utf8_lossy(&expected.stderr)
    );
    assert_eq!(expected.stdout, b"cfg-if 17 23 41 67 19 3\n");
    let mut db = SourceDb::new();
    let library = db.load_with_edition(&source, Edition::E2018).unwrap();
    let main = db.load_with_edition(&consumer, Edition::E2024).unwrap();
    let hir = flux_hir::lower_sources(
        &mut db,
        &[
            SourceCrate {
                root: library,
                name: Symbol::intern("cfg_if"),
                dependencies: vec![],
            },
            SourceCrate {
                root: main,
                name: Symbol::intern("consumer"),
                dependencies: vec![(Symbol::intern("cfg_if"), 0)],
            },
        ],
    );
    let record = |label: &str, errors: &[flux_hir::Diagnostic]| {
        let mut text = format!("count={}\n", errors.len());
        for d in errors {
            text.push_str(&format!(
                "{}:{:?}: {}\n",
                db.file(d.file).path.display(),
                d.range,
                d.message
            ));
        }
        std::fs::write(root.join(format!("{label}.diagnostics")), text).unwrap();
        assert!(errors.is_empty(), "{label}: {errors:?}");
    };
    record("syntax", &db.syntax_errors());
    record("hir", &hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    record("type", &typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    record("mir", &mir.diagnostics);
    for optimized in [false, true] {
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::Aarch64AppleDarwin;
        let tier = if optimized {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
        };
        let object = flux_codegen::compile_object(&mut module, target, tier).unwrap();
        let executable = root.join(format!("flux-{optimized}"));
        flux_codegen::link_executable(&object, target, &executable).unwrap();
        let actual = bounded(
            &mut Command::new(executable),
            &root,
            &format!("run-flux-{optimized}"),
        );
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
    for (path, bytes) in originals {
        assert_eq!(
            std::fs::read(&path).unwrap(),
            bytes,
            "{} changed",
            path.display()
        );
    }
    std::fs::write(root.join("source-integrity.json"), "[]\n").unwrap();
    std::fs::write(root.join("phase.txt"), "verified\n").unwrap();
}
