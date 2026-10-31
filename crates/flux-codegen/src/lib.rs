//! Code generation through keel, entirely in-process: MIR to keel IR
//! ([`lower_module`]), keel IR to a native object ([`compile_object`]),
//! and the object to an executable with keel's own linker
//! ([`link_executable`]). Linux uses the C driver to locate installed runtime
//! objects and libraries; linking stays in-process.
//!
//! Each MIR local becomes a keel variable and keel's `FunctionBuilder`
//! constructs SSA form, so MIR's many temporaries cost nothing. `bool` is
//! an `i8` holding 0 or 1, and `()` and `!` have no value at all.
//!
//! Rust functions get mangled symbol names, so they cannot collide with C
//! functions; the C `main` the platform calls is a small wrapper that
//! calls the Rust `main` and returns 0. A failed MIR assertion calls libc's
//! `abort`, as a panic does with `panic=abort`.

mod error;
pub use error::{Error, Stage};
mod linux;
mod lower;

use std::path::{Path, PathBuf};

pub use keel_ir::Target;
pub use keel_opt::OptimizationTier;
pub use lower::{
    Compilation, LoweredCrate, OutputKind, lower_crate, lower_crate_with_requirements, lower_module,
};

/// Verifies, optimizes and compiles `module` to an object file for
/// `target`.
pub fn compile_object(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
) -> Result<Vec<u8>, Error> {
    compile_object_inner(module, target, tier, None)
}

/// Completed or failed backend scopes. `None` means the scope was not reached.
/// These are not a total: error formatting and destruction of backend temporaries
/// can happen outside the measured calls.
#[derive(Default, Debug)]
pub struct ObjectTimings {
    pub verify: Option<std::time::Duration>,
    pub optimize: Option<std::time::Duration>,
    pub codegen: Option<std::time::Duration>,
    pub assemble: Option<std::time::Duration>,
}

/// The opt-in equivalent of [`compile_object`], retaining partial timings on errors.
/// The ordinary entry point performs no clock reads.
pub fn compile_object_with_timings(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
) -> (Result<Vec<u8>, Error>, ObjectTimings) {
    let mut timings = ObjectTimings::default();
    let result = compile_object_inner(module, target, tier, Some(&mut timings));
    (result, timings)
}

fn measured<T>(slot: Option<&mut Option<std::time::Duration>>, f: impl FnOnce() -> T) -> T {
    let Some(slot) = slot else { return f() };
    let start = std::time::Instant::now();
    let result = f();
    *slot = Some(start.elapsed());
    result
}

fn compile_object_inner(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
    mut timings: Option<&mut ObjectTimings>,
) -> Result<Vec<u8>, Error> {
    if let Err(errors) = measured(timings.as_deref_mut().map(|t| &mut t.verify), || {
        keel_ir::verify_module(module)
    }) {
        let lines: Vec<String> = errors.iter().map(|e| format!("  {e}")).collect();
        return Err(Error::message(
            Stage::Verify,
            format!(
                "internal error: flux produced invalid keel IR:\n{}\n{module}",
                lines.join("\n")
            ),
        ));
    }
    measured(timings.as_deref_mut().map(|t| &mut t.optimize), || {
        keel_opt::optimize_with_tier(module, tier)
    })
    .map_err(Error::optimize)?;
    let program = measured(timings.as_deref_mut().map(|t| &mut t.codegen), || {
        let selector = if tier == OptimizationTier::Full {
            keel_codegen::SelectionTier::Full
        } else {
            keel_codegen::SelectionTier::Baseline
        };
        keel_codegen::compile_program_with_pipeline(
            module,
            target,
            &keel_codegen::CodegenPipeline {
                selector: &selector,
                allocator: &keel_codegen::regalloc::AllocationTier::Full,
            },
        )
    })
    .map_err(Error::codegen)?;
    measured(timings.map(|t| &mut t.assemble), || {
        let object = match target {
            Target::Aarch64AppleDarwin => {
                keel_asm::assemble_program_object(&program, keel_asm::TargetFeatures::default())
            }
            Target::X86_64LinuxGnu => {
                let features =
                    keel_codegen::x86_features::module_features(module, Default::default())
                        .map_err(Error::codegen)?;
                keel_asm::x86::assemble_program_object_with_features(&program, features)
            }
            Target::Aarch64LinuxGnu => {
                return Err(Error::message(
                    Stage::Codegen,
                    "code generation for AArch64 Linux is not supported yet",
                ));
            }
            Target::Wasm32UnknownUnknown => {
                return Err(Error::message(
                    Stage::Codegen,
                    "code generation for WebAssembly is not supported yet",
                ));
            }
        };
        object.map_err(Error::assemble)
    })
}

/// Links `object` against the C library into an executable at `output`.
/// Returns the linker's warnings.
pub fn link_executable(object: &[u8], target: Target, output: &Path) -> Result<Vec<String>, Error> {
    link_executable_with_libraries(object, target, output, &[])
}

/// Links an object and the literal dynamic-library requirements collected
/// from the source graph. Object-only callers may use [`link_executable`].
pub fn link_executable_with_libraries(
    object: &[u8],
    target: Target,
    output: &Path,
    libraries: &[flux_intern::Symbol],
) -> Result<Vec<String>, Error> {
    let input = ObjectInput {
        name: "flux.o",
        bytes: object,
    };
    link_objects(&[input], target, output, libraries)
}

/// One native object supplied to the executable link.
#[derive(Debug, Clone, Copy)]
pub struct ObjectInput<'a> {
    /// A diagnostic label, such as the owning crate's object member name.
    pub name: &'a str,
    /// Relocatable native object bytes extracted from a validated artifact.
    pub bytes: &'a [u8],
}

/// Links native objects and requested system libraries into a native executable.
///
/// The caller supplies the executable object and each dependency object exactly
/// once. Inputs are object bytes, not `.flib` archives; artifact resolution and
/// dependency deduplication happen before this call. Returns linker warnings.
/// Supports native arm64 macOS and x86-64 Linux executable linking.
///
/// # Errors
///
/// Reports unsupported targets, SDK or system-library lookup failures, invalid
/// objects, unresolved or duplicate symbols, and output-writing failures.
pub fn link_objects(
    objects: &[ObjectInput<'_>],
    target: Target,
    output: &Path,
    libraries: &[flux_intern::Symbol],
) -> Result<Vec<String>, Error> {
    match target {
        Target::Aarch64AppleDarwin => {
            let sdk = macos_sdk()?;
            let system = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk)
                .map_err(|message| Error::message(Stage::Sdk, message))?;
            let mut dylibs = vec![system];
            let mut seen = std::collections::HashSet::new();
            for name in libraries {
                // Darwin's libc and libm are umbrella aliases of libSystem,
                // already included for the executable's runtime entry point.
                if matches!(name.as_str(), "c" | "m" | "System") || !seen.insert(*name) {
                    continue;
                }
                let path = sdk.join("usr/lib").join(format!("lib{name}.tbd"));
                let dylib = keel_link::Dylib::from_tbd(&path, &sdk)
                    .map_err(|message| Error::message(Stage::Sdk, message))?;
                dylibs.push(dylib);
            }
            let inputs: Vec<_> = objects
                .iter()
                .map(|o| keel_link::Input {
                    name: o.name,
                    bytes: o.bytes,
                })
                .collect();
            let executable = keel_link::link(
                &inputs,
                keel_link::Options {
                    dylibs,
                    ..keel_link::Options::default()
                },
            )
            .map_err(|message| Error::message(Stage::Link, message))?;
            keel_link::write_signed(&executable, output)
                .map_err(|message| Error::message(Stage::Write, message))?;
            Ok(executable.warnings)
        }
        Target::X86_64LinuxGnu => linux::link(objects, output, libraries),
        Target::Aarch64LinuxGnu => Err(Error::message(
            Stage::Link,
            "linking for AArch64 Linux is not supported yet",
        )),
        Target::Wasm32UnknownUnknown => Err(Error::message(
            Stage::Link,
            "linking for WebAssembly is not supported yet",
        )),
    }
}

/// The macOS SDK: `$SDKROOT`, or what `xcrun` reports.
fn macos_sdk() -> Result<PathBuf, Error> {
    if let Some(root) = std::env::var_os("SDKROOT").filter(|s| !s.is_empty()) {
        return Ok(PathBuf::from(root));
    }
    let output = std::process::Command::new("xcrun")
        .args(["--sdk", "macosx", "--show-sdk-path"])
        .output()
        .map_err(|error| Error::io(Stage::Sdk, "cannot find the macOS SDK (set SDKROOT)", error))?;
    if !output.status.success() {
        return Err(Error::message(
            Stage::Sdk,
            "cannot find the macOS SDK; set SDKROOT",
        ));
    }
    Ok(PathBuf::from(
        String::from_utf8_lossy(&output.stdout).trim(),
    ))
}
