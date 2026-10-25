//! Native threads exercise Flux-generated synchronization through audited ABI entries.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]

use keel_ir::{CallConv, Type};
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn bounded(dir: &Path, label: &str, command: &mut Command) -> Output {
    let out = dir.join(format!("{label}.stdout"));
    let err = dir.join(format!("{label}.stderr"));
    let mut child = command
        .stdout(Stdio::from(std::fs::File::create(&out).unwrap()))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap()))
        .spawn()
        .unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{label} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let output = Output {
        status,
        stdout: std::fs::read(out).unwrap(),
        stderr: std::fs::read(err).unwrap(),
    };
    assert!(
        output.status.success(),
        "{label}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[test]
fn generated_locks_and_lazy_initialization_match_rustc_under_contention() {
    let text = include_str!("fixtures/sync_runtime/native.rs.txt");
    let driver = include_str!("fixtures/sync_runtime/driver.rs.txt");
    let (_, _, hir) = flux_hir::lower_text("sync_runtime_native.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let mut original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
        "../../out/sync-runtime/native-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    // The same valid Rust worker source runs with rustc's locks as the oracle.
    let oracle = dir.join("oracle.rs");
    std::fs::write(&oracle, format!("mod implementation {{\n{text}\n}}\n{driver}\nfn main() {{ exercise(implementation::worker, implementation::totals); }}")).unwrap();
    let mut declarations = String::from("unsafe extern \"C\" {\n");
    for name in ["worker", "totals"] {
        let functions: Vec<_> = original
            .functions
            .iter_mut()
            .filter(|f| f.name.contains(name))
            .collect();
        assert_eq!(functions.len(), 1, "ambiguous {name}");
        let function = functions.into_iter().next().unwrap();
        assert_eq!(function.sig.cc, CallConv::C);
        assert!(function.sig.params.is_empty());
        assert_eq!(
            function
                .sig
                .returns
                .iter()
                .map(|p| p.ty)
                .collect::<Vec<_>>(),
            [Type::I64]
        );
        declarations.push_str(&format!(
            "#[link_name = {:?}] fn {name}() -> usize;\n",
            function.name
        ));
        function.local = false;
    }
    declarations.push_str("}\n");
    let host = dir.join("host.rs");
    std::fs::write(&host, format!("{declarations}{driver}\nfn call_worker() -> usize {{ unsafe {{ worker() }} }}\nfn call_totals() -> usize {{ unsafe {{ totals() }} }}\nfn main() {{ exercise(call_worker, call_totals); }}")).unwrap();
    for f in &mut original.functions {
        if f.name == "main" {
            f.name = "flux_unused_main".into();
        }
    }
    for tier in [
        keel_opt::OptimizationTier::None,
        keel_opt::OptimizationTier::Full,
    ] {
        let mut module = original.clone();
        keel_opt::optimize_with_tier(&mut module, tier).unwrap();
        keel_ir::verify_module(&module).unwrap();
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
        let object_path = dir.join(format!("{tier:?}.o"));
        std::fs::write(&object_path, object).unwrap();
        let mut results = Vec::new();
        for compiler in ["rustc", "flux"] {
            let exe = dir.join(format!("{compiler}-{tier:?}"));
            let mut cmd = Command::new("rustc");
            cmd.args(["--edition=2024", "-C", "panic=abort", "-o"])
                .arg(&exe);
            if tier == keel_opt::OptimizationTier::Full {
                cmd.arg("-O");
            }
            if compiler == "flux" {
                cmd.arg("-C")
                    .arg(format!("link-arg={}", object_path.display()))
                    .arg(&host);
            } else {
                cmd.arg(&oracle);
            }
            bounded(&dir, &format!("compile-{compiler}-{tier:?}"), &mut cmd);
            let result = bounded(
                &dir,
                &format!("run-{compiler}-{tier:?}"),
                &mut Command::new(&exe),
            );
            assert!(result.stderr.is_empty());
            assert_eq!(result.stdout, b"contention 1 1 8000 8000 16000\n");
            results.push(result.stdout);
        }
        assert_eq!(results[0], results[1]);
    }
}
