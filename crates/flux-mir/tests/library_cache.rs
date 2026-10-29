//! The embedded libraries' bodies are inferred, built and checked once
//! per process and shared by every compilation. Compilations on several
//! threads, started before anything is shared, give what each gives
//! alone afterwards.

use std::fmt::Write;
use std::hash::{Hash, Hasher};

const PROGRAMS: &[&str] = &[
    "fn main() { let v = vec![1, 2, 3]; println!(\"{}\", v.iter().sum::<i32>()); }\n",
    "fn main() { let s = String::from(\"a\"); let t = s.clone() + \"b\"; assert_eq!(t.len(), 2); }\n",
    "fn main() { let x: Option<u8> = Some(1); if let Some(v) = x.map(|v| v + 1) { println!(\"{v}\"); } }\n",
    "const N: usize = 2 + 2;\nfn main() { let a = [0u8; N]; println!(\"{}\", a.len()); }\n",
    "fn main() { let x: i32 = \"a\"; }\n",
];

/// Feeds what is written to a hasher, so that every body's MIR can be
/// compared without keeping its text.
struct HashWriter(std::collections::hash_map::DefaultHasher);

impl Write for HashWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        s.hash(&mut self.0);
        Ok(())
    }
}

/// The program's types and MIR as snapshots show them, every reached
/// body's MIR, `core`'s included, and the diagnostics.
fn compile(text: &str) -> String {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    let typed = flux_mir::check_file(&hir);
    let mut out = typed.debug_dump(&hir);
    if typed.diagnostics.is_empty() {
        let mir = flux_mir::build_file(&hir, &typed);
        out.push_str(&mir.debug_dump(&hir));
        let mut hash = HashWriter(Default::default());
        for (id, body) in mir.bodies.iter() {
            write!(hash, "{id:?} {body:?}").unwrap();
        }
        writeln!(
            out,
            "{} bodies, {:x}",
            mir.bodies.iter().count(),
            hash.0.finish()
        )
        .unwrap();
    } else {
        for d in flux_mir::check_clean_bodies(&hir, &typed, &typed.diagnostics) {
            writeln!(out, "{d:?}").unwrap();
        }
    }
    out
}

#[test]
fn parallel_compilations_share_the_libraries() {
    let threads = 4;
    let order = |thread: usize| (0..PROGRAMS.len()).map(move |i| (thread + i) % PROGRAMS.len());
    let parallel: Vec<Vec<String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|thread| {
                scope.spawn(move || order(thread).map(|p| compile(PROGRAMS[p])).collect())
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let alone: Vec<String> = PROGRAMS.iter().map(|text| compile(text)).collect();
    for (thread, results) in parallel.iter().enumerate() {
        for (p, result) in order(thread).zip(results) {
            assert_eq!(
                result, &alone[p],
                "thread {thread}, program:\n{}",
                PROGRAMS[p]
            );
        }
    }
}
