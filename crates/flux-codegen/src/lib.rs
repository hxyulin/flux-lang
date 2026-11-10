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

mod aarch64_asm;
mod error;
pub use error::{Error, Stage};
mod linux;
mod lower;
mod native;
mod profile;
mod units;

use std::path::{Path, PathBuf};

pub use keel_ir::{Module, Target};
pub use keel_opt::OptimizationTier;
pub use lower::{
    Compilation, LoweredCrate, LoweringTimings, OutputKind, lower_crate,
    lower_crate_with_requirements, lower_module, lower_partitioned_crate, lower_with_timings,
};
pub use native::{NativeLink, NativeLinkInputs};
pub use units::split_fast_module;

/// Verifies, optimizes and compiles `module` to an object file for
/// `target`.
pub fn compile_object(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
) -> Result<Vec<u8>, Error> {
    compile_object_inner(
        module,
        target,
        tier,
        flux_mir::PanicStrategy::Abort,
        None,
        None,
    )
}

/// Compiles an object with an explicit panic strategy.
///
/// Modules lowered from unwind MIR require `Unwind`, including modules whose
/// functions only forward calls. This retains unwind tables in ordinary frames.
/// The panic runtime and strategy-compatible dependency objects must be linked
/// by the caller. Unwind emission currently supports arm64 macOS only.
///
/// # Errors
/// Returns a staged error for an unsupported target, invalid IR, optimization,
/// native code generation or assembly.
pub fn compile_object_with_panic_strategy(
    module: &mut keel_ir::Module,
    target: Target,
    tier: OptimizationTier,
    panic_strategy: flux_mir::PanicStrategy,
) -> Result<Vec<u8>, Error> {
    compile_object_inner(module, target, tier, panic_strategy, None, None)
}

/// The IR and native selection policy of an object build.
#[derive(Clone, Copy, Debug)]
pub enum ObjectOptimization {
    /// The existing tier-based selection policy.
    Tier(OptimizationTier),
    /// Short IR optimization with full native instruction selection.
    FastBuild,
}
/// Object policy carried independently of optimization and timing observations.
#[derive(Clone, Copy, Debug)]
pub struct ObjectOptions {
    pub optimization: ObjectOptimization,
    pub panic_strategy: flux_mir::PanicStrategy,
}
/// Compiles an object with explicit policy and panic strategy.
///
/// # Errors
/// Returns staged verification, optimization, code generation or assembly errors.
pub fn compile_object_with_options(
    module: &mut Module,
    target: Target,
    options: ObjectOptions,
) -> Result<Vec<u8>, Error> {
    compile_options(module, target, options, None)
}
/// The timed equivalent of [`compile_object_with_options`].
pub fn compile_object_with_options_and_timings(
    module: &mut Module,
    target: Target,
    options: ObjectOptions,
) -> (Result<Vec<u8>, Error>, ObjectTimings) {
    let mut timings = ObjectTimings::default();
    let result = compile_options(module, target, options, Some(&mut timings));
    (result, timings)
}
fn compile_options(
    module: &mut Module,
    target: Target,
    options: ObjectOptions,
    timings: Option<&mut ObjectTimings>,
) -> Result<Vec<u8>, Error> {
    let (tier, selection) = match options.optimization {
        ObjectOptimization::Tier(tier) => (tier, None),
        ObjectOptimization::FastBuild => (
            OptimizationTier::Fast,
            Some(keel_codegen::SelectionTier::Full),
        ),
    };
    compile_object_inner(
        module,
        target,
        tier,
        options.panic_strategy,
        selection,
        timings,
    )
}

/// Uses the short IR pipeline with full native instruction selection.
/// This is the compiler's opt-in fast-build policy; the ordinary tier-based
/// entry point retains its existing selection policy.
///
/// # Errors
/// Returns a staged backend error for invalid IR or failed optimization,
/// native code generation or assembly.
pub fn compile_fast_object(module: &mut keel_ir::Module, target: Target) -> Result<Vec<u8>, Error> {
    compile_object_inner(
        module,
        target,
        OptimizationTier::Fast,
        flux_mir::PanicStrategy::Abort,
        Some(keel_codegen::SelectionTier::Full),
        None,
    )
}

/// Completed or failed backend scopes. `None` means the scope was not reached.
/// Codegen excludes encoding done by its chunk callback; assembly includes that
/// encoding and final object emission. A codegen failure can retain partial assembly
/// time when earlier chunks were encoded. These are not a total: error formatting
/// and destruction of backend temporaries
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
    let result = compile_object_inner(
        module,
        target,
        tier,
        flux_mir::PanicStrategy::Abort,
        None,
        Some(&mut timings),
    );
    (result, timings)
}

/// The timed equivalent of [`compile_fast_object`].
/// The result contains the same errors; timings retain completed scopes on failure.
pub fn compile_fast_object_with_timings(
    module: &mut keel_ir::Module,
    target: Target,
) -> (Result<Vec<u8>, Error>, ObjectTimings) {
    let mut timings = ObjectTimings::default();
    let result = compile_object_inner(
        module,
        target,
        OptimizationTier::Fast,
        flux_mir::PanicStrategy::Abort,
        Some(keel_codegen::SelectionTier::Full),
        Some(&mut timings),
    );
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
    panic_strategy: flux_mir::PanicStrategy,
    selection: Option<keel_codegen::SelectionTier>,
    mut timings: Option<&mut ObjectTimings>,
) -> Result<Vec<u8>, Error> {
    if panic_strategy == flux_mir::PanicStrategy::Unwind && target != Target::Aarch64AppleDarwin {
        return Err(Error::message(
            Stage::Codegen,
            "panic=unwind is supported only for arm64 macOS",
        ));
    }
    if panic_strategy == flux_mir::PanicStrategy::Abort
        && module
            .functions
            .iter()
            .any(|f| f.personality.as_deref() == Some(lower::UNWIND_PERSONALITY))
    {
        return Err(Error::message(
            Stage::Codegen,
            "unwind MIR requires panic=unwind object emission",
        ));
    }
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
        profile::optimize(module, tier)
    })?;
    let selector = selection.unwrap_or(if tier == OptimizationTier::Full {
        keel_codegen::SelectionTier::Full
    } else {
        keel_codegen::SelectionTier::Baseline
    });
    let pipeline = keel_codegen::CodegenPipeline {
        selector: &selector,
        allocator: &keel_codegen::regalloc::AllocationTier::Full,
    };
    let mut assembler = match target {
        Target::Aarch64AppleDarwin => {
            Some(keel_asm::ObjectAssembler::for_target(target).map_err(Error::assemble)?)
        }
        Target::X86_64LinuxGnu => {
            let features = keel_codegen::x86_features::module_features(module, Default::default())
                .map_err(Error::codegen)?;
            Some(
                keel_asm::ObjectAssembler::with_features(
                    target,
                    keel_asm::TargetFeatures::default(),
                    features,
                )
                .map_err(Error::assemble)?,
            )
        }
        Target::Aarch64LinuxGnu | Target::Wasm32UnknownUnknown => None,
    };
    let timed = timings.is_some();
    let mut encoding = std::time::Duration::ZERO;
    let mut emitted = false;
    let start = timed.then(std::time::Instant::now);
    let mut emission = keel_codegen::EmissionOptions::default();
    if panic_strategy == flux_mir::PanicStrategy::Unwind {
        emission.unwind_tables = keel_codegen::UnwindTables::All;
    }
    let result = keel_codegen::Request::new(target, pipeline)
        .emission(emission)
        .program_chunks(module, &mut |mut chunk| {
            if let Some(assembler) = &mut assembler {
                let start = timed.then(std::time::Instant::now);
                if target == Target::Aarch64AppleDarwin {
                    for item in &mut chunk.items {
                        if let keel_asm_ir::Item::Text(text) = item
                            && let std::borrow::Cow::Owned(rewritten) =
                                aarch64_asm::system_aliases(text)
                        {
                            *text = rewritten;
                        }
                    }
                }
                assembler.push(chunk);
                emitted = true;
                if let Some(start) = start {
                    encoding += start.elapsed();
                }
            }
        });
    if let (Some(timings), Some(start)) = (timings.as_deref_mut(), start) {
        // Encoding happens inside the codegen callback. Keep the two scopes exclusive.
        timings.codegen = Some(start.elapsed().saturating_sub(encoding));
        timings.assemble = emitted.then_some(encoding);
    }
    result.map_err(Error::codegen)?;
    let start = timed.then(std::time::Instant::now);
    let result = match assembler {
        Some(assembler) => assembler.finish().map_err(Error::assemble),
        None => Err(Error::message(
            Stage::Codegen,
            match target {
                Target::Aarch64LinuxGnu => "code generation for AArch64 Linux is not supported yet",
                Target::Wasm32UnknownUnknown => {
                    "code generation for WebAssembly is not supported yet"
                }
                _ => unreachable!("supported object targets have an assembler"),
            },
        )),
    };
    if let (Some(timings), Some(start)) = (timings, start) {
        timings.assemble = Some(encoding + start.elapsed());
    }
    result
}

/// Links `object` against the C library into an executable at `output`.
/// Returns the linker's warnings.
pub fn link_executable(object: &[u8], target: Target, output: &Path) -> Result<Vec<String>, Error> {
    link_executable_with_libraries(object, target, output, &NativeLink::default())
}

/// Links an object and the native libraries collected from the source graph
/// and its build scripts. Object-only callers may use [`link_executable`].
pub fn link_executable_with_libraries(
    object: &[u8],
    target: Target,
    output: &Path,
    native: &NativeLink,
) -> Result<Vec<String>, Error> {
    let input = ObjectInput {
        name: "flux.o",
        bytes: object,
    };
    link_objects(&[input], target, output, native)
}

/// Resolves native-library and platform inputs for executable freshness checks.
///
/// Repeating this resolution detects changes in search-path precedence. Dynamic
/// interfaces include their resolved exports and weak exports in `context`, so
/// transitive SDK reexports remain tracked without a second interface parser.
///
/// # Errors
///
/// Returns the same SDK, runtime-file and native-library resolution failures as
/// executable linking. A freshness caller can treat these failures as a miss.
pub fn native_link_inputs(target: Target, native: &NativeLink) -> Result<NativeLinkInputs, Error> {
    match target {
        Target::Aarch64AppleDarwin => Ok(native::darwin(native)?.inputs()),
        Target::X86_64LinuxGnu => linux::inputs(native),
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

/// One native object supplied to the executable link.
#[derive(Debug, Clone, Copy)]
pub struct ObjectInput<'a> {
    /// A diagnostic label, such as the owning crate's object member name.
    pub name: &'a str,
    /// Relocatable native object bytes extracted from a validated artifact.
    pub bytes: &'a [u8],
}

/// Links native objects and requested native libraries into a native executable.
///
/// The caller supplies the executable object and each dependency object exactly
/// once. Inputs are object bytes, not `.flib` archives; artifact resolution and
/// dependency deduplication happen before this call. Returns linker warnings.
/// Supports native arm64 macOS and x86-64 Linux executable linking.
///
/// # Errors
///
/// Reports unsupported targets, SDK or native-library lookup failures, invalid
/// objects, unresolved or duplicate symbols, and output-writing failures.
pub fn link_objects(
    objects: &[ObjectInput<'_>],
    target: Target,
    output: &Path,
    native: &NativeLink,
) -> Result<Vec<String>, Error> {
    link_objects_with_inputs(objects, target, output, native).map(|(warnings, _)| warnings)
}

/// Links an executable and reports the native inputs actually consumed.
///
/// The returned context includes digests of the retained archive/runtime bytes
/// passed to the linker, not a later filesystem read. Freshness callers can
/// compare it with pre-link and post-link inspections to reject input races.
///
/// # Errors
///
/// Returns the same linking and output-writing errors as [`link_objects`].
pub fn link_objects_with_inputs(
    objects: &[ObjectInput<'_>],
    target: Target,
    output: &Path,
    native: &NativeLink,
) -> Result<(Vec<String>, NativeLinkInputs), Error> {
    match target {
        Target::Aarch64AppleDarwin => {
            let resolved = native::darwin(native)?;
            let consumed = resolved.inputs();
            let native::DarwinLibraries {
                sdk_version,
                dylibs,
                archives,
                ..
            } = resolved;
            let names: Vec<_> = archives
                .iter()
                .map(|(path, _)| path.to_string_lossy().into_owned())
                .collect();
            let inputs: Vec<_> = objects
                .iter()
                .map(|o| keel_link::Input {
                    name: o.name,
                    bytes: o.bytes,
                })
                .chain(
                    names
                        .iter()
                        .zip(&archives)
                        .map(|(name, (_, bytes))| keel_link::Input { name, bytes }),
                )
                .collect();
            let defaults = keel_link::Options::default();
            let executable = keel_link::link(
                &inputs,
                keel_link::Options {
                    dylibs,
                    sdk_version,
                    // Functions nothing reaches are removed one by one.
                    dead_strip: true,
                    ..defaults
                },
            )
            .map_err(|message| Error::message(Stage::Link, message))?;
            keel_link::write_signed(&executable, output)
                .map_err(|message| Error::message(Stage::Write, message))?;
            Ok((executable.warnings, consumed))
        }
        Target::X86_64LinuxGnu => linux::link(objects, output, native),
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

/// The version of the macOS SDK at `sdk`, from its `SDKSettings.json`,
/// encoded as the build-version load command wants it: `xxxx.yy.zz` as
/// `0xXXXXYYZZ`.
fn sdk_version(sdk: &Path) -> Option<u32> {
    let settings = std::fs::read_to_string(sdk.join("SDKSettings.json")).ok()?;
    let after = &settings[settings.find("\"Version\"")? + "\"Version\"".len()..];
    let value = after
        .trim_start()
        .strip_prefix(':')?
        .trim_start()
        .strip_prefix('"')?;
    parse_version(&value[..value.find('"')?])
}

/// `27.0` or `15.4.1` as `0xXXXXYYZZ`.
fn parse_version(text: &str) -> Option<u32> {
    let mut parts = text.split('.').map(|part| part.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next().unwrap_or(Some(0))?;
    let patch = parts.next().unwrap_or(Some(0))?;
    (parts.next().is_none() && major <= 0xffff && minor <= 0xff && patch <= 0xff)
        .then_some(major << 16 | minor << 8 | patch)
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

#[cfg(test)]
mod tests {
    use super::parse_version;

    #[test]
    fn sdk_versions_encode_as_nibbles() {
        assert_eq!(parse_version("27.0"), Some(0x001b_0000));
        assert_eq!(parse_version("15.4.1"), Some(0x000f_0401));
        assert_eq!(parse_version("14"), Some(0x000e_0000));
        assert_eq!(parse_version("14.x"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.256"), None);
        assert_eq!(parse_version(""), None);
    }
}
