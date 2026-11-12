//! The explicit process guard also rejects an already initialized source cache.
#[path = "../../flux-cli/tests/support/process.rs"]
mod process;
#[path = "../../../tests/support/scratch.rs"]
mod scratch;

use flux_hir::Core;

#[test]
fn source_loading_guard_child() {
    let Ok(mode) = std::env::var("FLUX_BUILTIN_GUARD_CHILD") else {
        return;
    };
    if mode == "cached" {
        let _ = Core::get();
    }
    Core::forbid_source_loading();
    let _ = Core::get();
    panic!("source library guard did not stop access");
}

#[test]
fn explicit_guard_rejects_fresh_and_cached_source_library_access() {
    let scratch = scratch::Scratch::new("out/flib-builtin-guard");
    for mode in ["fresh", "cached"] {
        let output = process::run(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "source_loading_guard_child", "--nocapture"])
                .env("FLUX_BUILTIN_GUARD_CHILD", mode),
            scratch.path(),
            mode,
            std::time::Duration::from_secs(30),
        )
        .completed();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains("source builtin loading is forbidden after artifact selection"),
            "{stderr}"
        );
    }
    // Child guards do not prohibit source-based graph comparisons in this process.
    assert_eq!(Core::get().items.crates.len(), 4);
}
