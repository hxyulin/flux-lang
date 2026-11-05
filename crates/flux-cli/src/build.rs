//! `flux build` and `flux run`: the whole pipeline, from source to an
//! executable.

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use flux_codegen::{OptimizationTier, Target};
use flux_hir::SourceDb;
use flux_intern::Symbol;

use crate::{BuildOptions, report_all};

/// The crate name for a file: its stem, with anything that cannot appear
/// in an identifier replaced by `_`.
pub(crate) fn crate_name(file: &Path) -> String {
    let stem = file
        .file_stem()
        .map_or("main".into(), |s| s.to_string_lossy());
    let name: String = stem
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{name}")
    } else {
        name
    }
}

/// Opt-in scoped timings. Totals are elapsed wall time, never sums of scopes.
pub(crate) struct Timings {
    start: Option<Instant>,
    total: &'static str,
    phases: Vec<(&'static str, Duration)>,
}

impl Timings {
    pub(crate) fn new(enabled: bool, total: &'static str) -> Self {
        Self {
            start: enabled.then(Instant::now),
            total,
            phases: Vec::new(),
        }
    }

    pub(crate) fn record(&mut self, phase: &'static str, duration: Duration) {
        self.phases.push((phase, duration));
    }

    pub(crate) fn time<T>(&mut self, phase: &'static str, f: impl FnOnce() -> T) -> T {
        let start = self.start.map(|_| Instant::now());
        let result = f();
        if let Some(start) = start {
            self.phases.push((phase, start.elapsed()));
        }
        result
    }
}

impl Drop for Timings {
    fn drop(&mut self) {
        if let Some(start) = self.start {
            let total = start.elapsed();
            eprintln!("timings (nested scopes; do not sum):");
            for (phase, duration) in &self.phases {
                eprintln!("{phase:<38}  {duration:>10.2?}");
            }
            eprintln!("{:<38}  {total:>10.2?}", self.total);
        }
    }
}

/// Compiles `file` to an executable at `output`. `Ok(false)` means the
/// program had errors, which were reported.
pub(crate) fn build(file: &Path, output: &Path, options: BuildOptions) -> Result<bool, String> {
    compile(CompileRequest {
        file,
        crate_name: &crate_name(file),
        output,
        options,
        sources: &[],
    })
}

/// One compilation under Flux's host configuration; source crates retain their editions.
/// No Cargo profile, feature, target or rustc option is implied by this request.
pub(crate) struct CompileRequest<'a> {
    pub file: &'a Path,
    pub crate_name: &'a str,
    pub output: &'a Path,
    pub options: BuildOptions,
    /// Dependency-ordered source crates; empty for a single source file.
    pub sources: &'a [SourceInput],
}

pub(crate) struct SourceInput {
    pub host_macros: Option<Vec<flux_hir::ProcMacro>>,
    pub compilation_dir: Option<std::path::PathBuf>,
    pub edition: flux_syntax::Edition,
    pub file: std::path::PathBuf,
    pub name: String,
    /// Canonical manifest path, package name and declared version.
    pub package_identity: Option<(std::path::PathBuf, String, Option<String>)>,
    pub dependencies: Vec<(String, usize)>,
    pub cfg: flux_hir::CrateCfg,
    pub environment: Option<std::collections::BTreeMap<String, String>>,
}

impl SourceInput {
    pub(crate) fn standalone(file: &Path, name: &str) -> Self {
        Self {
            host_macros: None,
            compilation_dir: None,
            edition: flux_syntax::Edition::E2024,
            file: file.to_owned(),
            name: name.to_owned(),
            package_identity: None,
            dependencies: Vec::new(),
            cfg: flux_hir::CrateCfg::default(),
            environment: None,
        }
    }
}

// The scoped boundary includes diagnostics and destruction of compile-local data.
// It excludes CLI parsing, request construction, printing this timing report, final
// driver error formatting, program execution, and process/global teardown.
pub(crate) fn compile(request: CompileRequest<'_>) -> Result<bool, String> {
    let mut timings = Timings::new(request.options.timings, "compile scoped elapsed");
    compile_inner(request, &mut timings, None)
}

/// Compiles a generated root under its original physical path, preserving
/// relative modules, includes, and source locations without rewriting source.
pub(crate) fn compile_generated(request: CompileRequest<'_>, text: &str) -> Result<bool, String> {
    let mut timings = Timings::new(request.options.timings, "compile scoped elapsed");
    compile_inner(request, &mut timings, Some(text))
}

fn compile_inner(
    request: CompileRequest<'_>,
    timings: &mut Timings,
    generated: Option<&str>,
) -> Result<bool, String> {
    let CompileRequest {
        file,
        crate_name,
        output,
        options,
        sources,
    } = request;
    let suite = crate::builtins::active();
    // `core`'s files are parsed with the database.
    let (mut db, root) = timings.time("database + root load/parse", || {
        let mut db = suite.map_or_else(SourceDb::new, |s| s.db.fork());
        let edition = sources
            .last()
            .map_or(flux_syntax::Edition::E2024, |s| s.edition);
        let root = match generated {
            Some(text) => Ok(db.add_with_edition(file, text, edition)),
            None => db.load_with_edition(file, edition),
        };
        (db, root)
    });
    let root = root?;
    let graph = timings.time("source dependency load/parse", || {
        let mut graph = Vec::new();
        for source in sources {
            let source_root = if let Some(exports) = &source.host_macros {
                db.add_proc_macro_crate(source.file.clone(), exports.clone())
            } else if generated.is_some() && source.file == file {
                root
            } else {
                db.load_with_edition(&source.file, source.edition)?
            };
            db.set_crate_cfg(source_root, source.cfg.clone())?;
            if let Some(directory) = &source.compilation_dir {
                db.set_crate_directory(source_root, directory.clone())?;
            }
            if let Some(environment) = &source.environment {
                db.set_crate_env(source_root, environment.clone())?;
            }
            graph.push(flux_hir::SourceCrate {
                root: source_root,
                name: Symbol::intern(&source.name),
                dependencies: source
                    .dependencies
                    .iter()
                    .map(|(alias, target)| (Symbol::intern(alias), *target))
                    .collect(),
            });
        }
        Ok::<_, String>(graph)
    })?;
    let hir = timings.time("hir (includes module load/parse)", || {
        if let Some(suite) = suite {
            if graph.is_empty() {
                flux_hir::lower_source_on(
                    &mut db,
                    suite.items.clone(),
                    root,
                    Symbol::intern(crate_name),
                    Vec::new(),
                )
            } else {
                flux_hir::lower_sources_on(&mut db, suite.items.clone(), &graph)
            }
        } else if graph.is_empty() {
            flux_hir::lower(&mut db, root, Symbol::intern(crate_name))
        } else {
            flux_hir::lower_sources(&mut db, &graph)
        }
    });
    let providers = suite.map(|s| s.providers()).transpose()?;
    let roots = providers
        .as_ref()
        .map(|(loaded, imported)| imported.required_functions(&hir.items, loaded.signatures()));
    let typed = timings.time("types (includes const evaluation)", || {
        if let Some((loaded, imported)) = &providers {
            flux_mir::check_file_from(
                &hir,
                loaded,
                imported,
                &roots.as_ref().expect("provider roots").functions,
            )
        } else {
            flux_mir::check_file(&hir)
        }
    });
    let mut errors = db.syntax_errors();
    errors.extend(crate::identifiers::diagnostics(&db));
    errors.extend(hir.diagnostics.iter().cloned());
    errors.extend(typed.diagnostics.iter().cloned());
    if let Some(d) = flux_ty::check_main(&hir.items, &typed.sigs) {
        errors.push(d);
    }
    if !errors.is_empty() {
        let more = timings.time("mir checks on clean bodies", || {
            if let Some((_, imported)) = &providers {
                flux_mir::check_clean_bodies_from(&hir, &typed, &errors, imported)
            } else {
                flux_mir::check_clean_bodies(&hir, &typed, &errors)
            }
        });
        errors.extend(more);
    } else {
        let mir = timings.time("mir build + checks", || {
            if let Some((_, imported)) = &providers {
                flux_mir::build_file_from(
                    &hir,
                    &typed,
                    imported,
                    &roots.as_ref().expect("provider roots").functions,
                )
            } else {
                flux_mir::build_file(&hir, &typed)
            }
        });
        errors.extend(mir.diagnostics.iter().cloned());
        if errors.is_empty() {
            let target = Target::host()
                .ok_or("flux can only compile for arm64 macOS and x86-64 Linux hosts")?;
            let builtin_object = suite.is_some_and(|s| s.use_object(options.optimize));
            let hashes = if builtin_object {
                crate::builtins::graph_hashes(&hir.items)?
            } else {
                Vec::new()
            };
            let compilation = builtin_object.then(|| {
                let roots = roots.as_ref().expect("native suite has providers");
                flux_codegen::Compilation {
                    kind: flux_codegen::OutputKind::Executable,
                    crate_hashes: &hashes,
                    imported: &[],
                    required_functions: &roots.functions,
                    required_instances: &roots.instances,
                    builtin_object: true,
                }
            });
            let lowered = if options.timings {
                let mut parts = flux_codegen::LoweringTimings::default();
                let lowered = timings.time("mono + keel lowering (inclusive)", || {
                    flux_codegen::lower_with_timings(&hir, &typed, &mir, compilation, &mut parts)
                });
                let inclusive = timings.phases.last().map_or(Duration::ZERO, |p| p.1);
                let measured = parts.collect + parts.substitute + parts.validate;
                for (name, duration) in [
                    ("  mono collection", parts.collect),
                    ("  body substitution", parts.substitute),
                    ("  mir validation", parts.validate),
                    ("  keel lowering", inclusive.saturating_sub(measured)),
                ] {
                    timings.phases.push((name, duration));
                }
                lowered
            } else if let Some(compilation) = compilation {
                flux_codegen::lower_crate(&hir, &typed, &mir, compilation)
            } else {
                flux_codegen::lower_module(&hir, &typed, &mir)
            };
            let mut module = match lowered {
                Ok(module) => module,
                Err(found) => {
                    errors.extend(found);
                    report_all(&db, &mut errors);
                    return Ok(false);
                }
            };
            let tier = if options.optimize {
                OptimizationTier::Full
            } else {
                OptimizationTier::None
            };
            let object = if options.timings {
                let (result, backend) = timings.time("backend (inclusive)", || {
                    flux_codegen::compile_object_with_timings(&mut module, target, tier)
                });
                for (name, duration) in [
                    ("  backend verify", backend.verify),
                    ("  backend optimize", backend.optimize),
                    ("  backend native codegen", backend.codegen),
                    ("  backend assemble", backend.assemble),
                ] {
                    if let Some(duration) = duration {
                        timings.phases.push((name, duration));
                    }
                }
                result.map_err(|error| error.to_string())?
            } else {
                flux_codegen::compile_object(&mut module, target, tier)
                    .map_err(|error| error.to_string())?
            };
            let warnings = timings.time("link", || {
                if builtin_object {
                    let builtin = suite
                        .expect("native suite selected")
                        .object
                        .as_deref()
                        .expect("native object exists");
                    return flux_codegen::link_objects(
                        &[
                            flux_codegen::ObjectInput {
                                name: crate_name,
                                bytes: &object,
                            },
                            flux_codegen::ObjectInput {
                                name: "flux-builtins",
                                bytes: builtin,
                            },
                        ],
                        target,
                        output,
                        &hir.items.native_libraries,
                    )
                    .map_err(|error| error.to_string());
                }
                flux_codegen::link_executable_with_libraries(
                    &object,
                    target,
                    output,
                    &hir.items.native_libraries,
                )
                .map_err(|error| error.to_string())
            })?;
            for warning in warnings {
                eprintln!("warning: {warning}");
            }
        }
    }
    report_all(&db, &mut errors);
    Ok(errors.is_empty())
}

/// Builds `file` into a temporary executable and runs it with `args`,
/// returning its exit code (128 plus the signal if one killed it).
pub(crate) fn run(file: &Path, options: BuildOptions, args: &[String]) -> Result<ExitCode, String> {
    let output = std::env::temp_dir().join(format!(
        "flux-run-{}-{}",
        std::process::id(),
        crate_name(file)
    ));
    if !build(file, &output, options)? {
        return Ok(ExitCode::FAILURE);
    }
    let status = std::process::Command::new(&output).args(args).status();
    let _ = std::fs::remove_file(&output);
    let status = status.map_err(|e| format!("cannot run {}: {e}", output.display()))?;
    Ok(ExitCode::from(exit_code(status)))
}

pub(crate) fn exit_code(status: std::process::ExitStatus) -> u8 {
    if let Some(code) = status.code() {
        return code as u8;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128u8.wrapping_add(signal as u8);
        }
    }
    1
}
