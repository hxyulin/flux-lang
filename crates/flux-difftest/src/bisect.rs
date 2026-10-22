//! Telling flux's lowering from keel's optimizer for `-O`-only failures.
//!
//! `flux build -O` runs keel's full pipeline (`keel_opt::DEFAULT_PIPELINE`)
//! and an unoptimized build runs none of it. This builds a program through
//! flux's libraries, as `flux build` does, but runs only the first `n`
//! passes, so a binary search over `n` finds the first pass after which
//! the program behaves differently from rustc's. If the program already
//! fails with no passes, the bug is in flux's lowering (or keel's code
//! generator), not the optimizer.

use std::path::Path;

use flux_hir::SourceDb;
use flux_intern::Symbol;

/// keel's `-O` pipeline, pass by pass.
pub fn pipeline() -> &'static [&'static str] {
    keel_opt::DEFAULT_PIPELINE
}

/// flux's keel IR for `file`, before any optimization.
pub fn lower(file: &Path) -> Result<keel_ir::Module, String> {
    let mut db = SourceDb::new();
    let root = db.load(file)?;
    let hir = flux_hir::lower(&mut db, root, Symbol::intern("main"));
    let typed = flux_mir::check_file(&hir);
    if !db.syntax_errors().is_empty()
        || !hir.diagnostics.is_empty()
        || !typed.diagnostics.is_empty()
    {
        return Err("flux reports errors in the program".to_owned());
    }
    let mir = flux_mir::build_file(&hir, &typed);
    if !mir.diagnostics.is_empty() {
        return Err("flux reports errors in the program".to_owned());
    }
    flux_codegen::lower_module(&hir, &typed, &mir)
        .map_err(|_| "flux cannot lower the program".to_owned())
}

/// Runs the first `passes` passes of the pipeline on `module`.
pub fn optimize(module: &mut keel_ir::Module, passes: usize) -> Result<(), String> {
    keel_ir::verify_module(module).map_err(|e| format!("flux produced invalid keel IR: {e:?}"))?;
    keel_opt::run_passes(module, &pipeline()[..passes]).map_err(|e| e.to_string())
}

/// Builds `file` into the executable `out`, running the first `passes`
/// passes of keel's `-O` pipeline.
pub fn build(file: &Path, out: &Path, passes: usize) -> Result<(), String> {
    let mut module = lower(file)?;
    optimize(&mut module, passes)?;
    let target = keel_ir::Target::Aarch64AppleDarwin;
    let program = keel_codegen::compile_program(&module, target).map_err(|e| e.to_string())?;
    let object = keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
        .map_err(|e| e.to_string())?;
    flux_codegen::link_executable(&object, target, out)?;
    Ok(())
}
