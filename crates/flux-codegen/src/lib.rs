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

use std::path::{Path, PathBuf};

pub use keel_ir::Target;
pub use keel_opt::OptimizationTier;
pub use lower::{
    Compilation, LoweredCrate, LoweringTimings, OutputKind, lower_crate,
    lower_crate_with_requirements, lower_module, lower_with_timings,
};
pub use native::NativeLink;

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
        profile::optimize(module, tier)
    })?;
    let selector = if tier == OptimizationTier::Full {
        keel_codegen::SelectionTier::Full
    } else {
        keel_codegen::SelectionTier::Baseline
    };
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
    let result = keel_codegen::compile_program_chunks_with_pipeline(
        module,
        target,
        &pipeline,
        &mut |mut chunk| {
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
        },
    );
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
    match target {
        Target::Aarch64AppleDarwin => {
            let sdk = macos_sdk()?;
            let system = keel_link::Dylib::from_tbd(&sdk.join("usr/lib/libSystem.tbd"), &sdk)
                .map_err(|message| Error::message(Stage::Sdk, message))?;
            let mut dylibs = vec![system];
            let mut archives = Vec::new();
            let libraries = [sdk.join("usr/lib")];
            let frameworks = [sdk.join("System/Library/Frameworks")];
            for library in &native.libraries {
                // Darwin's libc and libm are umbrella aliases of libSystem,
                // already included for the executable's runtime entry point.
                if library.kind == flux_hir::NativeKind::Dylib
                    && matches!(library.name.as_str(), "c" | "m" | "System")
                {
                    continue;
                }
                // ld64's default `-search_paths_first` order.
                let found = native::find(
                    *library,
                    native,
                    &libraries,
                    &frameworks,
                    &["tbd", "dylib", "so", "a"],
                )?;
                let path = match found {
                    native::Found::Archive(path, bytes) => {
                        if !archives.iter().any(|(p, _)| *p == path) {
                            archives.push((path, bytes));
                        }
                        continue;
                    }
                    native::Found::Interface(path) => path,
                    native::Found::Shared(path) => {
                        return Err(Error::message(
                            Stage::Link,
                            format!(
                                "linking the Mach-O shared library {} is not supported yet; \
                                 the linker reads only `.tbd` interfaces",
                                path.display()
                            ),
                        ));
                    }
                };
                let dylib = keel_link::Dylib::from_tbd(&path, &sdk)
                    .map_err(|message| Error::message(Stage::Sdk, message))?;
                // Aliases such as libpthread name a library already present.
                if !dylibs.iter().any(|d| d.install_name == dylib.install_name) {
                    dylibs.push(dylib);
                }
            }
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
            // The SDK the executable was built against: Darwin picks some
            // runtime behavior by it, such as the malloc zone.
            let sdk_version = sdk_version(&sdk).unwrap_or(defaults.minimum_macos);
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
            Ok(executable.warnings)
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
