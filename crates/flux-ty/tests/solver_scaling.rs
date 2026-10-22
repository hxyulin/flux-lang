//! Diamond goal graphs must scale with their distinct requirements. Each
//! measurement runs in a killable subprocess: a regression must not hang the
//! test runner or exhaust its memory.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn diamond(levels: usize) -> String {
    let mut text =
        String::from("struct S; trait A0 {} trait B0 {} impl A0 for S {} impl B0 for S {}\n");
    for i in 1..=levels {
        for branch in ['A', 'B'] {
            text.push_str(&format!(
                "trait {branch}{i} {{}} impl<T: A{} + B{}> {branch}{i} for T {{}}\n",
                i - 1,
                i - 1,
            ));
        }
    }
    text.push_str(&format!(
        "trait Top {{ fn f(&self) {{}} }} impl<T: A{levels} + B{levels}> Top for T {{}}\n\
         fn main() {{ S.f(); }}\n"
    ));
    text
}

fn measure(levels: usize) -> Duration {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "solver_worker", "--ignored", "--nocapture"])
        .env("FLUX_SOLVER_LEVELS", levels.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if started.elapsed() > Duration::from_secs(20) {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("solver timed out at {levels} levels: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let nanos = stdout
        .lines()
        .find_map(|line| line.strip_prefix("SOLVER_NS="))
        .expect("worker timing")
        .parse()
        .unwrap();
    Duration::from_nanos(nanos)
}

#[test]
fn solver_diamond_scales() {
    for _ in 0..2 {
        let small = measure(12);
        let large = measure(48);
        let ratio = large.as_secs_f64() / small.max(Duration::from_millis(2)).as_secs_f64();
        println!("solver diamond: {small:?} at 12, {large:?} at 48 ({ratio:.2}x)");
        if ratio < 6.0 {
            return;
        }
    }
    panic!("four times the diamond levels took at least six times as long");
}

#[test]
#[ignore = "invoked by solver_diamond_scales in a bounded subprocess"]
fn solver_worker() {
    let levels = std::env::var("FLUX_SOLVER_LEVELS")
        .expect("run through solver_diamond_scales")
        .parse()
        .unwrap();
    let (_, _, hir) = flux_hir::lower_text("main.rs", &diamond(levels));
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let check = || {
        let typed = flux_ty::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    };
    check();
    let mut best = Duration::MAX;
    for _ in 0..5 {
        let start = Instant::now();
        check();
        best = best.min(start.elapsed());
    }
    println!("SOLVER_NS={}", best.as_nanos());
}
