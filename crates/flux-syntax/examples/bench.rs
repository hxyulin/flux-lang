//! Parser throughput by phase, over every `.rs` file under a directory.
//!
//! ```sh
//! cargo run --release -p flux-syntax --example bench -- ~/.cargo/registry/src [threads]
//! ```
//!
//! Files are read into memory first, so the timings exclude I/O. Besides
//! the parsing pipeline, this measures interning every identifier.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use flux_intern::Symbol;
use flux_syntax::parser::{self, Input};
use flux_syntax::{LexedStr, SyntaxKind};

#[derive(Default, Clone, Copy)]
struct Phases {
    lex: Duration,
    input: Duration,
    parse: Duration,
    build: Duration,
    tokens: usize,
    nodes: usize,
    tree_bytes: usize,
}

impl Phases {
    fn add(&mut self, other: Phases) {
        self.lex += other.lex;
        self.input += other.input;
        self.parse += other.parse;
        self.build += other.build;
        self.tokens += other.tokens;
        self.nodes += other.nodes;
        self.tree_bytes += other.tree_bytes;
    }
}

fn run(text: &str) -> Phases {
    let t0 = Instant::now();
    let lexed = LexedStr::new(text);
    let t1 = Instant::now();
    let input = Input::from_lexed(&lexed);
    let t2 = Instant::now();
    let output = parser::parse_source_file(&input);
    let t3 = Instant::now();
    let tokens = lexed.len();
    let (tree, _) = flux_syntax::build_tree(lexed, output);
    let t4 = Instant::now();
    let phases = Phases {
        lex: t1 - t0,
        input: t2 - t1,
        parse: t3 - t2,
        build: t4 - t3,
        tokens,
        nodes: tree.node_count(),
        tree_bytes: tree.heap_size(),
    };
    std::hint::black_box(tree);
    phases
}

/// The identifier and lifetime texts of a file, as the item tree will
/// intern them.
fn names(text: &str) -> Vec<&str> {
    let lexed = LexedStr::new(text);
    (0..lexed.len())
        .filter(|&i| {
            matches!(
                lexed.kind(i),
                SyntaxKind::IDENT | SyntaxKind::LIFETIME_IDENT
            )
        })
        .map(|i| lexed.token_text(i))
        .collect()
}

/// Runs `f` on every text, spread over `threads` threads that take files
/// from a shared counter, so large files do not unbalance the work.
fn parallel<T: Sync>(items: &[T], threads: usize, f: impl Fn(&T) + Sync) -> Duration {
    let next = AtomicUsize::new(0);
    let start = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    f(item);
                }
            });
        }
    });
    start.elapsed()
}

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = PathBuf::from(args.next().expect("usage: bench <dir> [threads]"));
    let threads: usize = args.next().map_or(1, |t| t.parse().expect("threads"));

    let mut paths = Vec::new();
    collect(&dir, &mut paths);
    paths.sort();
    let texts: Vec<String> = paths
        .iter()
        .filter_map(|p| std::fs::read_to_string(p).ok())
        .collect();
    let bytes: usize = texts.iter().map(String::len).sum();
    let mib = bytes as f64 / (1 << 20) as f64;
    println!("{} files, {mib:.1} MiB", texts.len());
    let rate = |d: Duration| mib / d.as_secs_f64();

    for text in texts.iter().take(200) {
        run(text);
    }
    let mut total = Phases::default();
    for text in &texts {
        total.add(run(text));
    }
    let sum = total.lex + total.input + total.parse + total.build;
    println!(
        "single thread: {sum:.2?} ({:.0} MiB/s), {} tokens",
        rate(sum),
        total.tokens
    );
    let phases = [
        ("lex", total.lex),
        ("input", total.input),
        ("parse", total.parse),
        ("build", total.build),
    ];
    for (name, d) in phases {
        println!(
            "  {name:<6} {d:>9.2?}  {:>5.1}%  {:>7.0} MiB/s  {:>5.1} ns/token",
            100.0 * d.as_secs_f64() / sum.as_secs_f64(),
            rate(d),
            d.as_nanos() as f64 / total.tokens as f64,
        );
    }
    println!(
        "trees: {} nodes ({:.2} per token), {:.1} MiB including the text ({:.2}x the source)",
        total.nodes,
        total.nodes as f64 / total.tokens as f64,
        total.tree_bytes as f64 / (1 << 20) as f64,
        total.tree_bytes as f64 / bytes as f64,
    );

    if threads > 1 {
        let wall = parallel(&texts, threads, |text| {
            std::hint::black_box(flux_syntax::parse_source_file(text));
        });
        println!(
            "{threads} threads, full parse: {wall:.2?} ({:.0} MiB/s)",
            rate(wall)
        );
    }

    // Interning: every identifier and lifetime, as the item tree will.
    let names: Vec<Vec<&str>> = texts.iter().map(|t| names(t)).collect();
    let count: usize = names.iter().map(Vec::len).sum();
    let per_name = |d: Duration| d.as_nanos() as f64 / count as f64;
    let intern_all = |file: &Vec<&str>| {
        for name in file {
            std::hint::black_box(Symbol::intern(name));
        }
    };
    let cold = parallel(&names, threads.max(1), intern_all);
    let stats = flux_intern::stats();
    println!(
        "intern {count} names, {threads} threads, cold: {cold:.2?} ({:.1} ns/name per thread); {} distinct, {:.1} MiB of arena",
        per_name(cold) * threads.max(1) as f64,
        stats.symbols,
        stats.arena_bytes as f64 / (1 << 20) as f64,
    );
    let warm = parallel(&names, 1, intern_all);
    println!(
        "intern {count} names, 1 thread, warm: {warm:.2?} ({:.1} ns/name)",
        per_name(warm)
    );
    if threads > 1 {
        let warm = parallel(&names, threads, intern_all);
        println!(
            "intern {count} names, {threads} threads, warm: {warm:.2?} ({:.1} ns/name per thread)",
            per_name(warm) * threads as f64
        );
    }
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}
