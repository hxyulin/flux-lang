//! Heavy, opt-in proof that a dirty Rust rebuild invalidates a staged suite.
//! Run alone with FLUX_TEST_CARGO selecting the local Cargo runner.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

#[path = "support/process.rs"]
mod process;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn git(repository: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .current_dir(repository)
        .args(args)
        .output()
        .unwrap();
    success(&output);
    output
}

fn copy_sources(repository: &Path, fixture: &Path) {
    let tracked = git(repository, &["ls-files", "-z"]);
    let files: Vec<_> = tracked
        .stdout
        .split(|&byte| byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| PathBuf::from(std::str::from_utf8(name).unwrap()))
        .collect();
    assert!(
        files
            .iter()
            .any(|path| path == Path::new("crates/flux-cli/src/builtins.rs")),
        "commit the built-in suite implementation before running this fixture"
    );
    for relative in files {
        let source_directory = relative.starts_with("crates") || relative.starts_with("library");
        if !source_directory
            && !matches!(
                relative.to_str().unwrap(),
                "Cargo.toml" | "Cargo.lock" | "rust-toolchain.toml"
            )
        {
            continue;
        }
        let target = fixture.join(&relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::copy(repository.join(relative), target).unwrap();
    }
    assert!(!fixture.join(".git").exists(), "fixture is not a worktree");
    assert!(
        !fixture.join("target").exists(),
        "fixture has its own fresh target"
    );
}

fn run(command: &mut Command, logs: &Path, label: &str, limit: Duration) -> Output {
    let output = process::run(command, logs, label, limit).completed();
    success(&output);
    output
}

fn build(fixture: &Path, logs: &Path, label: &str) {
    let cargo = std::env::var_os("FLUX_TEST_CARGO").unwrap_or_else(|| "cargo".into());
    run(
        Command::new(cargo)
            .current_dir(fixture)
            .env_remove("CARGO_TARGET_DIR")
            .args([
                "build",
                "--locked",
                "--offline",
                "-p",
                "flux-cli",
                "--bin",
                "flux",
            ]),
        logs,
        label,
        Duration::from_secs(900),
    );
}

fn compiler(executable: &Path, fixture: &Path) -> Command {
    let mut command = Command::new(executable);
    command
        .current_dir(fixture)
        .env_remove("FLUX_BUILTINS_SOURCE")
        .env_remove("FLUX_BUILTINS_MIR_ONLY")
        .env("FLUX_BUILTINS_TRACE", "1");
    command
}

#[test]
#[ignore = "two isolated compiler builds; run alone with FLUX_TEST_CARGO set"]
fn dirty_mir_source_rebuild_rejects_the_previously_staged_suite() {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let scratch = scratch::Scratch::new(repository.join("out/builtins-dirty-rebuild"));
    let fixture = scratch.path();
    copy_sources(&repository, fixture);
    let logs = fixture.join("out/rebuild-logs");
    let staged = fixture.join("out/staged");
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::create_dir_all(&staged).unwrap();
    let original_head = git(&repository, &["rev-parse", "HEAD"]).stdout;
    build(fixture, &logs, "initial-build");
    let built = fixture.join("target/debug/flux");
    let executable = staged.join("flux");
    std::fs::copy(&built, &executable).unwrap();
    run(
        compiler(&executable, fixture).arg("build-builtins"),
        &logs,
        "stage-suite",
        Duration::from_secs(120),
    );
    let suite = staged.join("flux-builtins.flib");
    let original_suite = std::fs::read(&suite).unwrap();
    let original_hash = flux_hir::flib::digest(&std::fs::read(&executable).unwrap());
    let source = fixture.join("main.rs");
    std::fs::write(
        &source,
        "const N: usize = 2 + 2; fn main() { let _: [u8; N] = [0; N]; }\n",
    )
    .unwrap();
    let output = run(
        compiler(&executable, fixture).arg("mir").arg(&source),
        &logs,
        "before-rebuild",
        Duration::from_secs(120),
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("flux builtins: suite"));

    // This reachable production diagnostic is compiled into the executable.
    // Only the isolated fixture changes; no commit or git worktree is created.
    let rust_source = fixture.join("crates/flux-mir/src/eval.rs");
    let text = std::fs::read_to_string(&rust_source).unwrap();
    let original = "\"constant without a value\"";
    let replacement = "\"constant without a value FLUX_DIRTY_SUITE_REBUILD\"";
    assert_eq!(text.matches(original).count(), 1);
    std::fs::write(&rust_source, text.replacen(original, replacement, 1)).unwrap();
    build(fixture, &logs, "dirty-rebuild");
    let rebuilt = std::fs::read(&built).unwrap();
    let marker = b"FLUX_DIRTY_SUITE_REBUILD";
    assert!(rebuilt.windows(marker.len()).any(|bytes| bytes == marker));
    assert_ne!(
        flux_hir::flib::digest(&rebuilt),
        original_hash,
        "a production MIR Rust edit must change the compiler identity"
    );
    assert_eq!(
        git(&repository, &["rev-parse", "HEAD"]).stdout,
        original_head,
        "the rebuild must not depend on a new git commit"
    );
    std::fs::copy(&built, &executable).unwrap();
    assert_eq!(std::fs::read(&suite).unwrap(), original_suite);
    let output = run(
        compiler(&executable, fixture).arg("mir").arg(&source),
        &logs,
        "after-rebuild",
        Duration::from_secs(120),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("flux builtins: source (suite missing or compiler changed)"),
        "{stderr}"
    );
    assert!(!stderr.contains("flux builtins: suite"), "{stderr}");
    assert!(
        !output.stdout.is_empty(),
        "source fallback must still compile the program"
    );
    assert_eq!(
        std::fs::read(&suite).unwrap(),
        original_suite,
        "source fallback must not silently rewrite the stale suite"
    );
}
