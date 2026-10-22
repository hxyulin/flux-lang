//! Exhaustive boolean columns must share their MIR continuations.

use std::process::Command;
use std::time::{Duration, Instant};

fn build(text: &str) -> (flux_hir::Hir, flux_mir::FileMir) {
    let (_, _, hir) = flux_hir::lower_text("main.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    (hir, mir)
}

#[test]
fn independent_boolean_columns() {
    // Run the whole probe in a subprocess: a regression must fail in bounded
    // time, before exponential construction can exhaust the test runner.
    const WORKER: &str = "FLUX_MIR_BOOLEAN_COLUMNS_WORKER";
    if std::env::var_os(WORKER).is_none() {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "independent_boolean_columns", "--nocapture"])
            .env(WORKER, "1")
            .spawn()
            .unwrap();
        let start = Instant::now();
        loop {
            if let Some(status) = child.try_wait().unwrap() {
                assert!(status.success(), "pattern scaling worker: {status}");
                return;
            }
            if start.elapsed() > Duration::from_secs(20) {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("MIR boolean pattern construction exceeded 20 seconds");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    for n in [8, 24, 96] {
        // Alternate the literal order, and include an outer binding and guard
        // so neither guard construction nor binding construction may expand.
        let pats: Vec<_> = (0..n)
            .map(|i| {
                if i % 2 == 0 {
                    "true | false"
                } else {
                    "false | true"
                }
            })
            .collect();
        let text = format!(
            "fn f(x: [bool; {n}], pass: bool) -> i32 {{
                match x {{ all @ [{}] if pass => if all[0] {{ 7 }} else {{ 9 }}, _ => 3 }}
             }} fn main() {{}}",
            pats.join(",")
        );
        let (hir, mir) = build(&text);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        // Count all emitted work, including unreachable blocks. A timing
        // threshold alone can hide small but exponentially growing graphs.
        let blocks: usize = mir
            .bodies
            .iter()
            .filter(|(id, _)| hir.is_program(*id))
            .map(|(_, b)| b.blocks.len())
            .sum();
        let statements: usize = mir
            .bodies
            .iter()
            .filter(|(id, _)| hir.is_program(*id))
            .flat_map(|(_, b)| b.blocks.iter())
            .map(|(_, b)| b.statements.len())
            .sum();
        println!("{n} columns: {blocks} blocks, {statements} statements");
        assert!(blocks < 4 * n + 100, "{n} columns: {blocks} blocks");
        assert!(
            statements < 4 * n + 100,
            "{n} columns: {statements} statements"
        );
    }
}

#[test]
fn exhaustive_literals_still_read_the_place() {
    for (text, expected) in [
        (
            "fn main() { let x: bool; match x { true | false => () } }",
            "isn't initialized",
        ),
        (
            "fn main() { let mut x = true; let r = &mut x;
             match x { false | true => () } *r = false; }",
            "mutably borrowed",
        ),
        (
            "fn f(p: &mut bool) { let r = &mut *p;
             match p { false | true => () } *r = true; } fn main() {}",
            "mutably borrowed",
        ),
        (
            "struct S(i32); fn f(x: (bool, S)) {
             match x { (true | false, s) => { let a = s; let b = s; } }
             } fn main() {}",
            "moved",
        ),
    ] {
        let (_, mir) = build(text);
        assert!(
            mir.diagnostics.iter().any(|d| d.message.contains(expected)),
            "expected {expected}: {:?}",
            mir.diagnostics
        );
    }
}
