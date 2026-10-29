//! Shared harness behavior, independent of compiler semantics.
mod support;
use std::process::Command;
use std::time::Duration;

#[test]
fn commands_preserve_output_failure_and_timeout_artifacts() {
    let root = std::env::temp_dir().join(format!("flux-test-support-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let result = support::process::run(
        Command::new("sh").args(["-c", "printf out; printf err >&2; exit 7"]),
        &root,
        "failure",
        Duration::from_secs(5),
    )
    .completed();
    assert_eq!(result.status.code(), Some(7));
    assert_eq!(result.stdout, b"out");
    assert_eq!(result.stderr, b"err");
    // exec prevents an extra process retaining output handles after timeout.
    let result = support::process::run(
        Command::new("sh").args(["-c", "exec sleep 10"]),
        &root,
        "timeout",
        Duration::from_millis(20),
    );
    assert!(result.timed_out);
    assert!(!result.output.status.success());
    assert!(
        std::fs::read_to_string(root.join("timeout.status"))
            .unwrap()
            .contains("timeout=true")
    );
    // Parent integrity checks can run after a timeout, before reporting failure.
    let source = root.join("source.rs");
    std::fs::write(&source, "before").unwrap();
    let files = vec![(source.clone(), b"before".to_vec())];
    std::fs::write(&source, "after").unwrap();
    assert_eq!(support::inventory::changed(&files), vec![&source]);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn cases_match_names_and_paths_without_broadening_the_filter() {
    assert!(support::cases::matches_filter("ok/borrow.rs", "borrow"));
    assert!(support::cases::matches_filter("owned", "borrow, owned"));
    assert!(!support::cases::matches_filter("borrow", "owned"));
    assert!(std::panic::catch_unwind(|| support::cases::matches_filter("x", " ")).is_err());
}

#[test]
fn scratch_directories_are_unique_and_kept_only_on_failure() {
    let parent = std::env::temp_dir().join("flux-test-support-scratch");
    let first = support::scratch::Scratch::new(&parent);
    let second = support::scratch::Scratch::new(&parent);
    assert_ne!(first.path(), second.path());
    let passed = first.path().to_path_buf();
    std::fs::write(passed.join("artifact"), "kept only on failure").unwrap();
    drop(first);
    assert!(!passed.exists());
    let failed = std::thread::spawn(move || {
        let path = second.path().to_path_buf();
        let _scratch = second;
        std::panic::panic_any(path);
    })
    .join()
    .unwrap_err();
    let failed = failed.downcast::<std::path::PathBuf>().unwrap();
    assert!(failed.is_dir());
    std::fs::remove_dir_all(*failed).unwrap();
}
