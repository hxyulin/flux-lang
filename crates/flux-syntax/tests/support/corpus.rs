//! The runner the corpus tests of every stage share (each includes this
//! file with `#[path]`): it runs a stage over every `.rs` file under
//! `FLUX_CORPUS` on every core, fails on panics, prints the slowest files,
//! and fails on any file the stage spent too long on for its size, which
//! is how quadratic and exponential slowdowns show up.
//!
//! A file's budget is a floor or a number of nanoseconds per byte the stage
//! read, whichever is more; ten times that in debug builds.
//! `FLUX_CORPUS_FLOOR_MS` and `FLUX_CORPUS_NS_PER_BYTE` replace a stage's
//! defaults. A file over its budget while every core was busy is timed again
//! alone, and fails only if it is still over. `FLUX_CORPUS_THREADS` sets
//! the number of threads, every core's by default.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// How long a stage may take on a file: `floor`, or `ns_per_byte` times
/// the bytes it read if that is more.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    pub floor: Duration,
    pub ns_per_byte: f64,
}

impl Budget {
    pub const fn new(floor_ms: u64, ns_per_byte: f64) -> Budget {
        Budget {
            floor: Duration::from_millis(floor_ms),
            ns_per_byte,
        }
    }

    /// This budget with the environment's overrides, and ten times more
    /// time in debug builds.
    fn with_env(self) -> Budget {
        let var = |name: &str| {
            let value = std::env::var(name).ok()?;
            let number = value.trim().parse::<f64>();
            Some(number.unwrap_or_else(|_| panic!("{name} must be a number, not {value:?}")))
        };
        let scale = if cfg!(debug_assertions) { 10.0 } else { 1.0 };
        let floor = var("FLUX_CORPUS_FLOOR_MS")
            .map_or(self.floor, |ms| Duration::from_secs_f64(ms / 1000.0));
        let ns_per_byte = var("FLUX_CORPUS_NS_PER_BYTE").unwrap_or(self.ns_per_byte);
        Budget {
            floor: floor.mul_f64(scale),
            ns_per_byte: ns_per_byte * scale,
        }
    }

    fn limit(&self, bytes: usize) -> Duration {
        let linear = Duration::from_secs_f64(bytes as f64 * self.ns_per_byte * 1e-9);
        self.floor.max(linear)
    }
}

/// Every `.rs` file under `FLUX_CORPUS`, sorted.
pub fn corpus_files() -> Vec<PathBuf> {
    let root =
        std::env::var_os("FLUX_CORPUS").expect("set FLUX_CORPUS to a directory of Rust files");
    let mut files = Vec::new();
    let mut stack = vec![PathBuf::from(root)];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    assert!(!files.is_empty(), "no .rs files under FLUX_CORPUS");
    files.sort();
    files
}

/// Runs `stage` on every file under `FLUX_CORPUS`; see [`run_files`].
pub fn for_each_corpus_file<R: Send>(
    what: &str,
    budget: Budget,
    stage: impl Fn(&Path, &str) -> (usize, R) + Sync,
) -> Vec<(PathBuf, R)> {
    run_files(what, &corpus_files(), Some(budget), stage)
}

/// One file's run: how long it took and how many bytes the stage read.
struct Timing {
    index: usize,
    time: Duration,
    bytes: usize,
}

/// Runs `stage` on each of `files` on every core, and returns what it
/// returned for each file it did not panic on, in order. `stage` returns
/// the number of bytes it read (more than the file's when it reads the
/// files of modules too) and its result.
///
/// Fails with the files it panicked on, and with those it spent longer on
/// than `budget` allows, timed again alone; prints the slowest files and
/// the one closest to its budget.
pub fn run_files<R: Send>(
    what: &str,
    files: &[PathBuf],
    budget: Option<Budget>,
    stage: impl Fn(&Path, &str) -> (usize, R) + Sync,
) -> Vec<(PathBuf, R)> {
    let budget = budget.map(Budget::with_env);
    let next = AtomicUsize::new(0);
    let timings: Mutex<Vec<Timing>> = Mutex::new(Vec::new());
    let results: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
    let panicked: Mutex<Vec<String>> = Mutex::new(Vec::new());
    let start = Instant::now();
    let threads = match std::env::var("FLUX_CORPUS_THREADS") {
        Ok(n) => n.parse().expect("FLUX_CORPUS_THREADS must be a number"),
        Err(_) => std::thread::available_parallelism().map_or(1, |n| n.get()),
    };
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = files.get(index) else {
                        break;
                    };
                    let Ok(text) = fs::read_to_string(path) else {
                        continue;
                    };
                    let file = Instant::now();
                    let run = || stage(path, &text);
                    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(run)) {
                        Ok((bytes, result)) => {
                            let time = file.elapsed();
                            timings.lock().unwrap().push(Timing { index, time, bytes });
                            results.lock().unwrap().push((index, result));
                        }
                        Err(_) => panicked.lock().unwrap().push(path.display().to_string()),
                    }
                }
            });
        }
    });
    let elapsed = start.elapsed();

    let mut timings = timings.into_inner().unwrap();
    timings.sort_by_key(|t| std::cmp::Reverse(t.time));
    let total: Duration = timings.iter().map(|t| t.time).sum();
    println!(
        "{what} {} files in {elapsed:.2?} ({total:.2?} on {threads} threads); slowest:",
        timings.len(),
    );
    for t in timings.iter().take(5) {
        println!(
            "{:>12.2?}  {:>9} B  {}",
            t.time,
            t.bytes,
            files[t.index].display()
        );
    }
    // The slowest per byte of the files that took long enough for their
    // size rather than a fixed cost to dominate: what the per-byte budget
    // is calibrated against.
    let per_byte = |t: &Timing| t.time.as_nanos() as f64 / t.bytes.max(1) as f64;
    let mut measurable: Vec<&Timing> = (timings.iter())
        .filter(|t| t.time >= Duration::from_millis(5))
        .collect();
    measurable.sort_by(|a, b| per_byte(b).total_cmp(&per_byte(a)));
    println!("slowest per byte of those over 5 ms:");
    for t in measurable.iter().take(3) {
        println!(
            "{:>9.0} ns/B  {:>9.2?}  {:>9} B  {}",
            per_byte(t),
            t.time,
            t.bytes,
            files[t.index].display()
        );
    }

    let mut slow = Vec::new();
    if let Some(budget) = budget {
        let share = |t: &Timing| t.time.as_secs_f64() / budget.limit(t.bytes).as_secs_f64();
        if let Some(t) = timings.iter().max_by(|a, b| share(a).total_cmp(&share(b))) {
            println!(
                "closest to its budget ({:.2?} or {:.0} ns/B): {:.0}% ({:.2?}, {} B)  {}",
                budget.floor,
                budget.ns_per_byte,
                100.0 * share(t),
                t.time,
                t.bytes,
                files[t.index].display()
            );
        }
        for t in timings.iter().filter(|t| t.time > budget.limit(t.bytes)) {
            let path = &files[t.index];
            let limit = budget.limit(t.bytes);
            // Under load, a file can take several times as long as it
            // does alone: time it again with nothing else running.
            let mut best = t.time;
            for _ in 0..3 {
                let Ok(text) = fs::read_to_string(path) else {
                    break;
                };
                let file = Instant::now();
                stage(path, &text);
                best = best.min(file.elapsed());
                if best <= limit || best > 2 * limit {
                    break;
                }
            }
            if best > limit {
                slow.push(format!(
                    "{what}: {} ({} B) took {best:.2?}, over its budget of {limit:.2?} \
                     ({:.2?} or {:.0} ns/B): slower than linear in its size?",
                    path.display(),
                    t.bytes,
                    budget.floor,
                    budget.ns_per_byte,
                ));
            }
        }
    }

    let panicked = panicked.into_inner().unwrap();
    assert!(
        panicked.is_empty(),
        "{what}: panicked on:\n{}",
        panicked.join("\n")
    );
    assert!(
        slow.is_empty(),
        "\n{}\n(FLUX_CORPUS_FLOOR_MS and FLUX_CORPUS_NS_PER_BYTE change the budget)",
        slow.join("\n")
    );
    let mut results = results.into_inner().unwrap();
    results.sort_by_key(|r| r.0);
    results
        .into_iter()
        .map(|(index, result)| (files[index].clone(), result))
        .collect()
}
