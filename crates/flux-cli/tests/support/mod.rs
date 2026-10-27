//! Shared mechanics for compiler integration tests; graph policy stays in each suite.
#![allow(
    dead_code,
    reason = "each integration binary uses a different subset of the shared harness"
)]
pub mod inventory;
pub mod oracle;
pub mod process;

#[path = "../../../../tests/support/cases.rs"]
pub mod cases;
