//! Shrinking a failing program while it fails the same way.
//!
//! Two phases:
//!
//! 1. **Choices.** A generated program is a function of its choice tape
//!    (see [`crate::rng`]), and every tape gives a valid program. The
//!    reducer deletes runs of choices, zeroes them and lowers single
//!    ones, regenerating the program each time and keeping a change if
//!    the program got smaller and still has the same failure
//!    [`Signature`]. This removes whole functions, statements, branches
//!    and subexpressions while the program stays valid by construction.
//! 2. **Lines.** Then it deletes runs of lines and whole `{ ... }`
//!    blocks from the text, keeping a deletion if rustc still accepts the
//!    program and it still fails the same way (so the result stays
//!    valid Rust), and formats the result with rustfmt.
//!
//! Candidates are tested in parallel, in batches of one per worker; the
//! first in order that works is taken.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::driver::{
    Mode, Signature, Tools, flux_compile_signature, is_compile_time, test_program,
};
use crate::generator::{Config, generate};
use crate::rng::Source;

pub struct Reducer {
    pub tools: Tools,
    pub cfg: Config,
    pub target: Signature,
    pub dir: PathBuf,
    pub jobs: usize,
    pub verbose: bool,
    tests: AtomicUsize,
    cache: Mutex<HashMap<u64, bool>>,
}

/// How big a program is: shorter text first, then fewer nonzero choices.
fn size(text: &str, tape: &[u32]) -> (usize, usize) {
    (text.len(), tape.iter().filter(|&&v| v != 0).count())
}

fn hash(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    h.finish()
}

impl Reducer {
    pub fn new(tools: Tools, cfg: Config, target: Signature, dir: PathBuf, jobs: usize) -> Reducer {
        Reducer {
            tools,
            cfg,
            target,
            dir,
            jobs: jobs.max(1),
            verbose: false,
            tests: AtomicUsize::new(0),
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// How many programs were compiled and run.
    pub fn tests(&self) -> usize {
        self.tests.load(Ordering::Relaxed)
    }

    /// Whether `text` fails with the target signature (worker `w`'s
    /// files).
    pub fn interesting(&self, text: &str, w: usize) -> bool {
        let key = hash(text);
        if let Some(&r) = self.cache.lock().unwrap().get(&key) {
            return r;
        }
        let dir = self.dir.join(format!("w{w}"));
        let _ = std::fs::create_dir_all(&dir);
        self.tests.fetch_add(1, Ordering::Relaxed);
        // A compile-time failure must show in flux alone first: cheaper
        // than rustc, and most candidates fail here.
        let r = if is_compile_time(&self.target)
            && flux_compile_signature(&self.tools, &dir, "candidate", text).as_ref()
                != Some(&self.target)
        {
            false
        } else {
            let v = test_program(&self.tools, &dir, "candidate", text, &Mode::BOTH);
            v.signature().as_ref() == Some(&self.target)
        };
        self.cache.lock().unwrap().insert(key, r);
        r
    }

    /// The first candidate (in order) that is interesting, testing them
    /// in parallel batches.
    fn first_interesting(&self, candidates: &[String]) -> Option<usize> {
        for (b, batch) in candidates.chunks(self.jobs).enumerate() {
            let found: Vec<bool> = std::thread::scope(|s| {
                let handles: Vec<_> = batch
                    .iter()
                    .enumerate()
                    .map(|(w, text)| s.spawn(move || self.interesting(text, w)))
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            if let Some(i) = found.iter().position(|&f| f) {
                return Some(b * self.jobs + i);
            }
        }
        None
    }

    /// Phase 1: shrinks the choice tape. Returns the smallest tape found
    /// and its program.
    pub fn reduce_tape(&self, tape: Vec<u32>) -> (Vec<u32>, String) {
        let (mut text, src) = generate(Source::replay(tape), &self.cfg);
        let mut tape = src.tape().to_vec();
        for round in 0..6 {
            let before = size(&text, &tape);
            for op in [Op::Delete, Op::Zero, Op::Lower] {
                let mut chunk = (tape.len() / 2).max(1);
                loop {
                    self.tape_pass(op, chunk, &mut tape, &mut text);
                    if chunk == 1 || (op != Op::Delete && chunk <= 2) {
                        break;
                    }
                    // Single choices only once the tape is short.
                    let floor = if tape.len() > 600 { 4 } else { 1 };
                    if chunk / 2 < floor {
                        break;
                    }
                    chunk /= 2;
                    if op == Op::Lower {
                        break;
                    }
                }
            }
            if self.verbose {
                eprintln!(
                    "reduce: round {round}: {} bytes, {} choices, {} tests",
                    text.len(),
                    tape.len(),
                    self.tests()
                );
            }
            if size(&text, &tape) == before {
                break;
            }
        }
        (tape, text)
    }

    /// One pass of `op` with runs of `chunk` choices over the tape.
    fn tape_pass(&self, op: Op, chunk: usize, tape: &mut Vec<u32>, text: &mut String) {
        let mut i = 0;
        while i < tape.len() {
            // Candidates from position i on, as many as one batch.
            let mut cands: Vec<(usize, Vec<u32>, String)> = Vec::new();
            let mut j = i;
            while j < tape.len() && cands.len() < self.jobs {
                let at = j;
                j += if op == Op::Lower { 1 } else { chunk };
                let Some(t) = op.apply(tape, at, chunk) else {
                    continue;
                };
                let (cand_text, src) = generate(Source::replay(t), &self.cfg);
                let used = src.tape().to_vec();
                if size(&cand_text, &used) < size(text, tape) {
                    cands.push((at, used, cand_text));
                }
            }
            if cands.is_empty() {
                i = j;
                continue;
            }
            let texts: Vec<String> = cands.iter().map(|c| c.2.clone()).collect();
            match self.first_interesting(&texts) {
                Some(k) => {
                    let (at, t, s) = cands.swap_remove(k);
                    *tape = t;
                    *text = s;
                    i = at;
                }
                None => i = j,
            }
        }
    }

    /// Phase 2: deletes lines and blocks while the program stays valid and
    /// fails the same way.
    pub fn reduce_lines(&self, text: String) -> String {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        for _round in 0..10 {
            let before = lines.len();
            // Whole blocks: a line opening a brace through the line
            // closing it.
            let mut i = 0;
            while i < lines.len() {
                let mut cands = Vec::new();
                let mut spans = Vec::new();
                let mut j = i;
                while j < lines.len() && cands.len() < self.jobs {
                    if lines[j].trim_end().ends_with('{')
                        && let Some(end) = matching_close(&lines, j)
                    {
                        let mut l = lines.clone();
                        l.drain(j..=end);
                        cands.push(l.join("\n") + "\n");
                        spans.push(j);
                    }
                    j += 1;
                }
                match self.first_interesting(&cands) {
                    Some(k) => {
                        let at = spans[k];
                        let end = matching_close(&lines, at).unwrap();
                        lines.drain(at..=end);
                        i = at;
                    }
                    None => i = j,
                }
            }
            // Runs of lines.
            let mut chunk = (lines.len() / 4).max(1);
            loop {
                let mut i = 0;
                while i < lines.len() {
                    let mut cands = Vec::new();
                    let mut spans = Vec::new();
                    let mut j = i;
                    while j < lines.len() && cands.len() < self.jobs {
                        let end = (j + chunk).min(lines.len());
                        let mut l = lines.clone();
                        l.drain(j..end);
                        cands.push(l.join("\n") + "\n");
                        spans.push((j, end));
                        j = end;
                    }
                    match self.first_interesting(&cands) {
                        Some(k) => {
                            let (a, b) = spans[k];
                            lines.drain(a..b);
                            i = a;
                        }
                        None => i = j,
                    }
                }
                if chunk == 1 {
                    break;
                }
                chunk /= 2;
            }
            if self.verbose {
                eprintln!(
                    "reduce: lines: {} lines, {} tests",
                    lines.len(),
                    self.tests()
                );
            }
            if lines.len() == before {
                break;
            }
        }
        lines.join("\n") + "\n"
    }

    /// Formats `text` with rustfmt, if that keeps it failing.
    pub fn format(&self, text: &str) -> String {
        let path = self.dir.join("format.rs");
        if std::fs::write(&path, text).is_err() {
            return text.to_owned();
        }
        let ok = std::process::Command::new("rustfmt")
            .args(["--edition", "2024"])
            .arg(&path)
            .status()
            .is_ok_and(|s| s.success());
        if !ok {
            return text.to_owned();
        }
        match std::fs::read_to_string(&path) {
            Ok(formatted) if self.interesting(&formatted, 0) => formatted,
            _ => text.to_owned(),
        }
    }
}

/// The line closing the brace that line `open` ends with.
fn matching_close(lines: &[String], open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (k, line) in lines.iter().enumerate().skip(open) {
        for c in line.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 {
            return (k > open).then_some(k);
        }
    }
    None
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Op {
    /// Delete `chunk` choices.
    Delete,
    /// Set `chunk` choices to 0.
    Zero,
    /// Lower one choice: halve it.
    Lower,
}

impl Op {
    fn apply(self, tape: &[u32], at: usize, chunk: usize) -> Option<Vec<u32>> {
        let end = (at + chunk).min(tape.len());
        let mut t = tape.to_vec();
        match self {
            Op::Delete => {
                t.drain(at..end);
            }
            Op::Zero => {
                if t[at..end].iter().all(|&v| v == 0) {
                    return None;
                }
                t[at..end].fill(0);
            }
            Op::Lower => {
                if t[at] <= 1 {
                    return None;
                }
                t[at] /= 2;
            }
        }
        Some(t)
    }
}

/// Reduces the program of `seed` (or the file's text, when there is no
/// tape) and returns the result.
pub fn reduce(reducer: &Reducer, tape: Option<Vec<u32>>, text: String) -> String {
    let text = match tape {
        Some(t) => reducer.reduce_tape(t).1,
        None => text,
    };
    let text = reducer.reduce_lines(text);
    reducer.format(&text)
}

/// A scratch directory for a reduction.
pub fn scratch(base: &Path, name: &str) -> PathBuf {
    let d = base.join("reduce").join(name);
    let _ = std::fs::create_dir_all(&d);
    d
}
