//! Code generation through keel, entirely in-process: MIR to keel IR
//! ([`lower_module`]), keel IR to a native object ([`compile_object`]),
//! and the object to an executable with keel's own linker
//! ([`link_executable`]). No C compiler or system linker is involved.
//!
//! Each MIR local becomes a keel variable and keel's `FunctionBuilder`
//! constructs SSA form, so MIR's many temporaries cost nothing. `bool` is
//! an `i8` holding 0 or 1, and `()` and `!` have no value at all.
//!
//! Rust functions get mangled symbol names, so they cannot collide with C
//! functions; the C `main` the platform calls is a small wrapper that
//! calls the Rust `main` and returns 0. A failed MIR assertion calls libc's
//! `abort`, as a panic does with `panic=abort`.

mod lower;

use std::path::{Path, PathBuf};

pub use keel_ir::Target;
pub use keel_opt::OptimizationTier;
pub use lower::lower_module;

/// Verifies, optimizes and compiles `module` to an object file for
/// `target`.
pub fn compile_object(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
) -> Result<Vec<u8>, String> {
    if let Err(errors) = keel_ir::verify_module(module) {
        let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        return Err(format!(
            "internal error: flux produced invalid keel IR:\n{}\n{module}",
            lines.join("\n")
        ));
    }
    keel_opt::optimize_with_tier(module, tier).map_err(|e| e.to_string())?;
    let program = keel_codegen::compile_program(module, target).map_err(|e| e.to_string())?;
    let object = match target {
        Target::Aarch64AppleDarwin => {
            keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
        }
        Target::X86_64LinuxGnu => keel_asm::x86::assemble_program_object(&program),
    };
    object.map_err(|e| e.to_string())
}

/// Links `object` against the C library into an executable at `output`.
/// Returns the linker's warnings.
pub fn link_executable(
    object: &[u8],
    target: Target,
    output: &Path,
) -> Result<Vec<String>, String> {
    match target {
        Target::Aarch64AppleDarwin => {
            let sdk = macos_sdk()?;
            let system = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk)?;
            let input = keel_link::Input {
                name: "flux.o",
                bytes: object,
            };
            let executable = keel_link::link(
                &[input],
                keel_link::Options {
                    dylibs: vec![system],
                    ..keel_link::Options::default()
                },
            )?;
            keel_link::write_signed(&executable, output)?;
            Ok(executable.warnings)
        }
        Target::X86_64LinuxGnu => Err("linking for x86-64 Linux is not supported yet".to_owned()),
    }
}

/// The macOS SDK: `$SDKROOT`, or what `xcrun` reports.
fn macos_sdk() -> Result<PathBuf, String> {
    if let Some(root) = std::env::var_os("SDKROOT").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .map_err(|e| format!("cannot find the macOS SDK (set SDKROOT): {e}"))?;
    if !output.status.success() {
        return Err("cannot find the macOS SDK; set SDKROOT".to_owned());
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}
