//! Inference scaling probes run in a child so a regression cannot hang the harness.

#[path = "../../flux-syntax/tests/support/scaling.rs"]
mod scaling;

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[test]
fn independent_closure_captures() {
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "closure_capture_worker", "--nocapture"])
        .env("FLUX_INFERENCE_CAPTURE_WORKER", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(45);
    loop {
        if child.try_wait().unwrap().is_some() {
            let output = child.wait_with_output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(output.status.success(), "{stdout}\n{stderr}");
            println!("{stdout}");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("closure capture inference exceeded 45s: {output:?}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn closure_capture_worker() {
    if std::env::var_os("FLUX_INFERENCE_CAPTURE_WORKER").is_none() {
        return;
    }
    scaling::assert_scales(
        "independent closure captures",
        4000,
        |n| {
            let bindings = scaling::repeat(n, |i| format!("let x{i} = {i}i32;"));
            let uses = scaling::repeat(n, |i| format!("let _ = &x{i};"));
            let source = format!("fn main() {{ {bindings} let _c = || {{ {uses} }}; }}");
            let (_, _, hir) = flux_hir::lower_text("main.rs", &source);
            assert!(hir.diagnostics.is_empty());
            (hir, n)
        },
        |(hir, n)| {
            let typed = flux_ty::check_file(hir);
            assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
            let captures: usize = typed
                .bodies
                .iter()
                .filter(|(f, _)| {
                    hir.items.functions[*f].name == flux_intern::Symbol::intern("main")
                })
                .flat_map(|(_, body)| body.closures.iter())
                .map(|(_, info)| info.captures.len())
                .sum();
            assert_eq!(captures, *n);
        },
    );
}

#[test]
fn interleaved_capture_groups_preserve_semantics() {
    use flux_ty::{CaptureMode, CaptureProj, Mutability};

    let source = include_str!("data/ok/inference_capture_groups.rs");
    let (_, _, hir) = flux_hir::lower_text("main.rs", source);
    assert!(hir.diagnostics.is_empty());
    let typed = flux_ty::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let main = hir
        .items
        .functions
        .iter()
        .find(|(_, f)| f.name == flux_intern::Symbol::intern("main"))
        .unwrap()
        .0;
    let body = &hir.body(main).unwrap().0;
    let info = typed.bodies[main].closures.iter().next().unwrap().1;
    let actual: Vec<_> = info
        .captures
        .iter()
        .map(|c| {
            (
                body.bindings[c.binding].name.to_string(),
                c.projection.to_vec(),
                c.mode,
            )
        })
        .collect();
    let shared = CaptureMode::ByRef(Mutability::Not);
    let mutable = CaptureMode::ByRef(Mutability::Mut);
    assert_eq!(
        actual,
        vec![
            ("b".into(), vec![CaptureProj::Field(0)], shared),
            ("b".into(), vec![CaptureProj::Field(1)], mutable),
            ("a".into(), vec![], mutable),
            ("c".into(), vec![], shared),
        ]
    );
}
