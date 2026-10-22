//! The check the scaling tests of every stage share (each includes this
//! file with `#[path]`): a stage given four times the input takes less
//! than six times as long, so quadratic and exponential slowdowns fail a
//! plain `cargo test` rather than waiting for the corpus. Linear stages
//! measure 3.8 to 4.2 times; a quadratic term fails once it is a fifth of
//! the linear one at `n`, so sizes are chosen to make it that large with
//! the slowdowns found so far put back.

#![allow(dead_code)]

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Times below this are noise: a run at `n` counts as taking at least
/// this long.
const FLOOR: Duration = Duration::from_millis(2);

/// The most four times the input may take: linear gives 4, quadratic 16.
const LIMIT: f64 = 6.0;

/// Fails if `run` on `make(4 * n)` takes more than [`LIMIT`] times as long
/// as on `make(n)`, the best of three runs each. A failure is measured
/// again, with more runs, before it counts.
pub fn assert_scales<T>(what: &str, n: usize, make: impl Fn(usize) -> T, run: impl Fn(&T)) {
    // One at a time: tests run in parallel, and would slow each other.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());
    let _guard = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
    let (small, large) = (make(n), make(4 * n));
    // Warm up: `core`, the interner and the allocator.
    run(&small);
    let mut ratio = 0.0;
    let mut times = (Duration::ZERO, Duration::ZERO);
    for runs in [3, 7] {
        times = (Duration::MAX, Duration::MAX);
        for _ in 0..runs {
            times.0 = times.0.min(time(|| run(&small)));
            times.1 = times.1.min(time(|| run(&large)));
        }
        ratio = times.1.as_secs_f64() / times.0.max(FLOOR).as_secs_f64();
        println!(
            "{what}: {:.2?} at n = {n}, {:.2?} at 4n ({ratio:.1}x)",
            times.0, times.1
        );
        if ratio < LIMIT {
            return;
        }
    }
    panic!(
        "{what}: {:.2?} at n = {n} but {:.2?} at 4n = {}, {ratio:.1}x for four times \
         the input (at most {LIMIT}x): slower than linear?",
        times.0,
        times.1,
        4 * n
    );
}

fn time(f: impl FnOnce()) -> Duration {
    let start = Instant::now();
    f();
    start.elapsed()
}

/// `n` copies of `item(i)`, one per line.
pub fn repeat(n: usize, item: impl Fn(usize) -> String) -> String {
    let mut out = String::new();
    for i in 0..n {
        out.push_str(&item(i));
        out.push('\n');
    }
    out
}
