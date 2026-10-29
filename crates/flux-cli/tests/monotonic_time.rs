//! Monotonic clock and normalized duration contracts used by assembler profiling.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
mod support;
extern crate alloc as alloc_crate;
pub use core::{fmt, ops, str};

#[path = "../../../library/core/src/time.rs"]
#[allow(
    dead_code,
    reason = "the host oracle checks production duration arithmetic"
)]
mod duration_implementation;

#[test]
fn monotonic_time_matches_rustc() {
    support::oracle::compare_fixture("monotonic_time");
}

// Compile the actual implementation with the host as an independent oracle.
#[path = "../../../library/std/src/time.rs"]
#[allow(
    dead_code,
    reason = "host checks exercise duration arithmetic; native fixture exercises clock methods"
)]
mod implementation;

#[test]
fn duration_arithmetic_matches_std() {
    use duration_implementation::Duration;
    use std::time::Duration as Std;

    let values = [
        (0, 0),
        (0, 1),
        (0, 999_999_999),
        (1, 0),
        (1, u32::MAX),
        (u64::MAX / 2, 500_000_000),
        (u64::MAX - 1, 999_999_999),
        (u64::MAX, 0),
        (u64::MAX, 999_999_999),
    ];
    for (secs, nanos) in values {
        let actual = Duration::new(secs, nanos);
        let expected = Std::new(secs, nanos);
        assert_eq!(actual.as_nanos(), expected.as_nanos());
        assert_eq!(
            actual.as_secs_f64().to_bits(),
            expected.as_secs_f64().to_bits()
        );
        for (other_secs, other_nanos) in values {
            let other = Duration::new(other_secs, other_nanos);
            let std_other = Std::new(other_secs, other_nanos);
            assert_eq!(
                actual.checked_add(other).map(|d| d.as_nanos()),
                expected.checked_add(std_other).map(|d| d.as_nanos()),
                "add ({secs}, {nanos}) + ({other_secs}, {other_nanos})"
            );
            assert_eq!(
                actual.checked_sub(other).map(|d| d.as_nanos()),
                expected.checked_sub(std_other).map(|d| d.as_nanos()),
                "sub ({secs}, {nanos}) - ({other_secs}, {other_nanos})"
            );
        }
    }
    for nanos in [1_000_000_000, u32::MAX] {
        assert!(std::panic::catch_unwind(|| Duration::new(u64::MAX, nanos)).is_err());
        assert!(std::panic::catch_unwind(|| Std::new(u64::MAX, nanos)).is_err());
    }
}
