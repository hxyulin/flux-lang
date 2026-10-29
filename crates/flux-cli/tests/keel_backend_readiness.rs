//! Audits and executes the complete pinned backend source graph.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "support/backend_graph.rs"]
mod backend_graph;
mod support;

#[test]
#[ignore = "backend source/input audit; run explicitly with --ignored --nocapture"]
fn unchanged_backend_readiness() {
    backend_graph::run_suite(backend_graph::Suite {
        test: "unchanged_backend_readiness",
        output: "keel-backend-readiness",
        harness: "crates/flux-cli/tests/keel_backend_readiness.rs",
        fixture: "crates/flux-cli/tests/fixtures/keel_backend/consumer.rs.txt",
        prepare: None,
        default_scope: None,
        programs: &[],
    });
}
