#[path = "../support/process.rs"]
mod process;

/// Runs test subprocesses with a deadline and persistent, file-backed diagnostics.
pub fn run(command: &mut std::process::Command, label: &str) -> std::process::Output {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../out/flib-tests")
        .join(format!(
            "{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
    std::fs::create_dir_all(&directory).unwrap();
    let result = process::run(
        command,
        &directory,
        label,
        std::time::Duration::from_secs(90),
    );
    assert!(
        !result.timed_out,
        "command timed out; see {}",
        directory.display()
    );
    result.completed()
}
