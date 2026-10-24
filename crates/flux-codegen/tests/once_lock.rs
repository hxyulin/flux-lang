//! Native host threads call audited C-scalar Flux function-pointer entries.
#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use keel_ir::{CallConv, InstData, Ordering, Type};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn bounded(dir: &Path, label: &str, command: &mut Command) {
    let err = dir.join(format!("{label}.stderr"));
    let mut child = command
        .stdout(Stdio::from(
            std::fs::File::create(dir.join(format!("{label}.stdout"))).unwrap(),
        ))
        .stderr(Stdio::from(std::fs::File::create(&err).unwrap()))
        .spawn()
        .unwrap();
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(
                status.success(),
                "{label}: {}",
                String::from_utf8_lossy(&std::fs::read(&err).unwrap())
            );
            return;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("{label} timed out");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn native_contention_and_publication() {
    let text = include_str!("fixtures/once_lock/native.rs.txt");
    let (_, _, hir) = flux_hir::lower_text("once_lock_native.rs", text);
    assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
    let typed = flux_mir::check_file(&hir);
    assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
    let mir = flux_mir::build_file(&hir, &typed);
    assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
    let mut original = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../out/once-lock/native");
    std::fs::create_dir_all(&dir).unwrap();
    let mut decls = String::from("unsafe extern \"C\" {\n");
    for (name, params, ret, args, rust_ret) in [
        ("make_cell", vec![], Type::Ptr, "", "*mut u8"),
        (
            "probe_cell",
            vec![Type::Ptr],
            Type::I64,
            "p: *mut u8",
            "usize",
        ),
        (
            "worker_cell",
            vec![Type::Ptr, Type::Ptr, Type::I64],
            Type::I64,
            "p: *mut u8, control: *mut usize, seed: usize",
            "usize",
        ),
        (
            "free_cell",
            vec![Type::Ptr],
            Type::I64,
            "p: *mut u8",
            "usize",
        ),
    ] {
        let found: Vec<_> = original
            .functions
            .iter_mut()
            .filter(|f| f.name.contains(name))
            .collect();
        assert_eq!(found.len(), 1);
        let f = found.into_iter().next().unwrap();
        assert_eq!(f.sig.cc, CallConv::C);
        assert_eq!(
            f.sig.params.iter().map(|p| p.ty).collect::<Vec<_>>(),
            params
        );
        assert_eq!(
            f.sig.returns.iter().map(|p| p.ty).collect::<Vec<_>>(),
            [ret]
        );
        decls.push_str(&format!(
            "#[link_name = {:?}] fn {name}({args}) -> {rust_ret};\n",
            f.name
        ));
        f.local = false;
    }
    decls.push_str("}\n");
    std::fs::write(
        dir.join("driver.rs"),
        format!(
            "{decls}{}",
            include_str!("fixtures/once_lock/driver.rs.txt")
        ),
    )
    .unwrap();
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
        let mut load = false;
        let mut cas = false;
        let mut store = false;
        for f in &module.functions {
            for (_, inst) in f.layout_insts() {
                match f.inst_data(inst) {
                    InstData::AtomicLoad {
                        ordering: Ordering::Acquire,
                        ..
                    } => load = true,
                    InstData::AtomicCas {
                        ordering: Ordering::Acquire,
                        ..
                    } => cas = true,
                    InstData::AtomicStore {
                        ordering: Ordering::Release,
                        ..
                    } => store = true,
                    _ => {}
                }
            }
        }
        assert!(load && cas && store);
        std::fs::write(dir.join(format!("{tier:?}.ir")), module.to_string()).unwrap();
        let object = flux_codegen::compile_object(
            &mut module,
            flux_codegen::Target::Aarch64AppleDarwin,
            keel_opt::OptimizationTier::None,
        )
        .unwrap();
        let object_path = dir.join(format!("{tier:?}.o"));
        std::fs::write(&object_path, object).unwrap();
        let exe = dir.join(format!("driver-{tier:?}"));
        let mut cmd = Command::new("rustc");
        cmd.args(["--edition=2024", "-C", "opt-level=2", "-C"])
            .arg(format!("link-arg={}", object_path.display()))
            .arg(dir.join("driver.rs"))
            .arg("-o")
            .arg(&exe);
        bounded(&dir, &format!("compile-{tier:?}"), &mut cmd);
        bounded(&dir, &format!("run-{tier:?}"), &mut Command::new(exe));
    }
}
