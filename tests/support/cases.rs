//! Selection shared by fixture loops; an unset filter always runs every case.
use std::path::Path;

/// Match any comma-separated substring against a stable case name.
pub fn selected(name: &str) -> bool {
    let Ok(filter) = std::env::var("FLUX_TEST_CASE") else {
        return true;
    };
    let selected = matches_filter(name, &filter);
    if selected {
        eprintln!("flux-case: {name}");
    }
    selected
}

pub fn path_selected(path: &Path) -> bool {
    selected(&path.to_string_lossy().replace('\\', "/"))
}

/// The outer runner owns parallelism; direct Cargo runs retain four workers.
pub fn workers() -> usize {
    std::env::var("FLUX_TEST_JOBS").map_or(4, |value| {
        value
            .parse::<usize>()
            .ok()
            .filter(|&n| n > 0)
            .expect("FLUX_TEST_JOBS must be a positive integer")
    })
}

/// Pure matching also lets the harness test selection without mutating process env.
pub fn matches_filter(name: &str, filter: &str) -> bool {
    assert!(
        !filter.trim().is_empty(),
        "FLUX_TEST_CASE must not be empty"
    );
    assert!(
        filter.split(',').all(|part| !part.trim().is_empty()),
        "FLUX_TEST_CASE contains an empty pattern"
    );
    filter.split(',').any(|part| {
        let part = part.trim();
        assert!(!part.is_empty(), "FLUX_TEST_CASE contains an empty pattern");
        name.contains(part)
    })
}
