//! Partitioned fast builds preserve the existing cross-object Rust ABI.
#![cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
mod support;

use std::process::Command;
use std::time::Duration;

#[test]
fn partitioned_aggregate_calls_data_vtables_and_drop_match_rustc() {
    compare_partitioned_with_rustc(include_str!("../../../tests/run/backend_partition_abi.rs"));
}

#[test]
fn partitioned_private_constant_graphs_match_rustc() {
    compare_partitioned_with_rustc(include_str!("../../../tests/run/private_constants.rs"));
}

fn compare_partitioned_with_rustc(text: &str) {
    let scratch = support::scratch::Scratch::new(std::env::temp_dir().join("flux-backend-units"));
    let root = scratch.path();
    let source = root.join("main.rs");
    std::fs::write(&source, text).unwrap();
    let reference = root.join("reference");
    let output = Command::new("rustc")
        .arg(&source)
        .args(["--edition=2024", "-O", "-o"])
        .arg(&reference)
        .output()
        .unwrap();
    support::oracle::success(&output, "rustc");
    let expected = Command::new(&reference).output().unwrap();
    support::oracle::success(&expected, "reference");
    for units in [1, 4, 8] {
        let executable = root.join(format!("units-{units}"));
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("build")
                .arg(&source)
                .args(["--fast-build", "-o"])
                .arg(&executable)
                .env("FLUX_BACKEND_UNITS", units.to_string()),
            root,
            "compile",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "partitioned compiler");
        if units > 1 {
            assert!(
                String::from_utf8_lossy(&output.stderr)
                    .contains(&format!("backend units: {units}, workers: 1"))
            );
        }
        let actual = Command::new(&executable).output().unwrap();
        assert_eq!(actual.status.code(), expected.status.code());
        assert_eq!(actual.stdout, expected.stdout);
        assert_eq!(actual.stderr, expected.stderr);
    }
}

#[test]
fn native_graph_reserves_root_budget_and_invalidates_unit_policy() {
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-backend-unit-budget"));
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='unit-budget'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname='unit-budget'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        include_str!("../../../tests/run/backend_partition_abi.rs"),
    )
    .unwrap();
    for (units, fresh) in [(8, false), (8, true), (4, false), (4, true)] {
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args([
                    "build",
                    "--offline",
                    "--fast-build",
                    "-j2",
                    "--manifest-path",
                ])
                .arg(root.join("Cargo.toml"))
                .env("FLUX_BACKEND_UNITS", units.to_string()),
            root,
            "build",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "native graph");
        let stderr = String::from_utf8_lossy(&output.stderr);
        if fresh {
            assert!(stderr.contains("Fresh unit_budget"), "{stderr}");
        } else {
            assert!(
                stderr.contains(&format!("backend units: {units}, workers: 2")),
                "{stderr}"
            );
        }
        let actual = Command::new(root.join("target/flux/fast/unit-budget"))
            .output()
            .unwrap();
        support::oracle::success(&actual, "partitioned executable");
        assert_eq!(actual.stdout, b"partitioned 108 243 42 42\ndropped\n");
    }
}

#[test]
fn persistent_units_reuse_unchanged_code_and_reject_corrupt_objects() {
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-backend-unit-cache"));
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='unit-cache'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname='unit-cache'\nversion='0.1.0'\n",
    )
    .unwrap();
    let build = |source: &str, expected: &[u8]| {
        std::fs::write(root.join("src/main.rs"), source).unwrap();
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args([
                    "build",
                    "--offline",
                    "--fast-build",
                    "-j2",
                    "--manifest-path",
                ])
                .arg(root.join("Cargo.toml"))
                .env("FLUX_BACKEND_UNITS", "8")
                .env("FLUX_BACKEND_CACHE", "1"),
            root,
            "build",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "cached units");
        let actual = Command::new(root.join("target/flux/fast/unit-cache"))
            .output()
            .unwrap();
        support::oracle::success(&actual, "cached executable");
        assert_eq!(actual.stdout, expected);
        String::from_utf8(output.stderr).unwrap()
    };
    let source = "fn helper()->i32{7} fn main(){println!(\"{}\",helper());}\n";
    let first = build(source, b"7\n");
    assert!(first.contains("backend units reused: 0/8"), "{first}");
    // A physical edit forces the driver past executable freshness. Unit reuse
    // must survive the source revision and arena identity changes.
    let second = build(&format!("{source}// physical edit\n"), b"7\n");
    let hits = second
        .lines()
        .find_map(|line| line.strip_prefix("backend units reused: "))
        .unwrap();
    assert!(
        hits.split('/').next().unwrap().parse::<usize>().unwrap() > 0,
        "{second}"
    );
    std::fs::write(
        root.join("target/flux/fast/unit-cache.backend-units/unit-0.o"),
        b"corrupt",
    )
    .unwrap();
    let third = build(&format!("{source}// another physical edit\n"), b"7\n");
    assert!(!third.contains("backend units reused: 8/8"), "{third}");
    let changed = source.replace("{7}", "{19}");
    let fourth = build(&changed, b"19\n");
    assert!(!fourth.contains("backend units reused: 8/8"), "{fourth}");
}

#[test]
fn private_literal_edits_replace_only_the_consuming_cached_unit() {
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-unit-private-literal"));
    let root = scratch.path();
    let source = root.join("main.rs");
    let executable = root.join("program");
    for (index, value) in ["first", "other", "first"].iter().enumerate() {
        std::fs::write(&source, format!("fn main() {{ println!(\"{value}\"); }}\n")).unwrap();
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_fluxc"))
                .arg("build")
                .arg(&source)
                .args(["--fast-build", "-o"])
                .arg(&executable)
                .env("FLUX_BACKEND_UNITS", "8")
                .env("FLUX_BACKEND_CACHE", "1"),
            root,
            "compile",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "literal edit");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = if index == 0 { "0/8" } else { "7/8" };
        assert!(
            stderr.contains(&format!("backend units reused: {expected}")),
            "{stderr}"
        );
        let actual = Command::new(&executable).output().unwrap();
        support::oracle::success(&actual, "literal executable");
        assert_eq!(actual.stdout, format!("{value}\n").as_bytes());
        // Each slot has one object and one record, including after undo.
        assert_eq!(
            std::fs::read_dir(root.join("program.backend-units"))
                .unwrap()
                .count(),
            16
        );
    }
}

#[test]
fn native_callback_objects_survive_archive_changes() {
    let scratch =
        support::scratch::Scratch::new(std::env::temp_dir().join("flux-unit-native-callback"));
    let root = scratch.path();
    std::fs::create_dir(root.join("src")).unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname='unit-native'\nversion='0.1.0'\nedition='2024'\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.lock"),
        "version=4\n[[package]]\nname='unit-native'\nversion='0.1.0'\n",
    )
    .unwrap();
    std::fs::write(root.join("build.rs"), "fn main(){println!(\"cargo:rustc-link-search=native={}\",std::env::var(\"CARGO_MANIFEST_DIR\").unwrap());}").unwrap();
    std::fs::write(
        root.join("src/main.rs"),
        r#"
#[link(name="callback",kind="static")]
unsafe extern "C" { fn native_value()->i32; }
fn private_part(value:(i32,i32))->i32 { value.0 + value.1 }
#[unsafe(no_mangle)]
pub extern "C" fn rust_part()->i32 { private_part((3,4)) }
fn main(){
    // SAFETY: the fixture archive defines this function with the C ABI.
    println!("{}", unsafe { native_value() });
}
"#,
    )
    .unwrap();
    for value in [5, 9] {
        std::fs::write(root.join("callback.c"), format!("extern int rust_part(void); int native_value(void) {{ return {value} + rust_part(); }}")).unwrap();
        for (tool, args) in [
            ("cc", vec!["-c", "callback.c", "-o", "callback.o"]),
            ("ar", vec!["crs", "libcallback.a", "callback.o"]),
        ] {
            let output = Command::new(tool)
                .args(args)
                .current_dir(root)
                .output()
                .unwrap();
            support::oracle::success(&output, tool);
        }
        let output = support::process::run(
            Command::new(env!("CARGO_BIN_EXE_flux"))
                .args([
                    "build",
                    "--offline",
                    "--fast-build",
                    "-j2",
                    "--manifest-path",
                ])
                .arg(root.join("Cargo.toml"))
                .env("FLUX_BACKEND_UNITS", "8")
                .env("FLUX_BACKEND_CACHE", "1"),
            root,
            "build",
            Duration::from_secs(120),
        )
        .completed();
        support::oracle::success(&output, "native callback build");
        if value == 9 {
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("backend units reused: 8/8"), "{stderr}");
        }
        let actual = Command::new(root.join("target/flux/fast/unit-native"))
            .output()
            .unwrap();
        support::oracle::success(&actual, "native callback");
        assert_eq!(actual.stdout, format!("{}\n", value + 7).as_bytes());
    }
}
