//! Dependency-ordered Fluxlib builds sharing immutable compiler-owned libraries.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use flux_hir::{ProcMacro, ProcMacroKind, ProcMacroResolver};
use flux_intern::Symbol;
use serde::{Deserialize, Serialize};

use crate::build::SourceInput;
use crate::flib::semantic::FreshArtifacts;
use crate::flib_build::specialization_object::Specializations;
use crate::flib_build::{ArtifactOutput, ExternalLibraries};
use crate::{BuildOptions, CrateType};

#[derive(Deserialize, Serialize)]
struct Macro {
    name: String,
    kind: String,
    helpers: Option<Vec<String>>,
    descriptor: Vec<u8>,
}

/// Reports a failed job during unwinding too, so the scheduler can stop
/// waiting before the thread scope propagates the worker's panic.
pub(super) struct JobCompletion<T> {
    pub(super) sender: std::sync::mpsc::Sender<(usize, Result<T, String>)>,
    pub(super) index: usize,
    pub(super) result: Option<Result<T, String>>,
}

impl<T> Drop for JobCompletion<T> {
    fn drop(&mut self) {
        let result = self
            .result
            .take()
            .unwrap_or_else(|| Err("compiler unit panicked".into()));
        let _ = self.sender.send((self.index, result));
    }
}

#[derive(Deserialize, Serialize)]
struct Request {
    file: PathBuf,
    name: String,
    edition: String,
    directory: Option<PathBuf>,
    package: Option<(PathBuf, String, Option<String>)>,
    cfg: Vec<String>,
    environment: Option<BTreeMap<String, String>>,
    macros: Option<Vec<Macro>>,
    externs: Vec<(String, PathBuf)>,
    output: PathBuf,
    library: bool,
    metadata_only: bool,
    backend_jobs: usize,
    optimize: bool,
    fast_build: bool,
    #[serde(default)]
    optimize_build_script: bool,
    debug_assertions: bool,
    panic_strategy: String,
    timings: bool,
    native: crate::native_link::Request,
}

impl Request {
    fn new(source: &SourceInput, options: BuildOptions) -> Result<Self, String> {
        let mut cfg: Vec<_> = source.cfg.names.iter().map(ToString::to_string).collect();
        cfg.extend(source.cfg.values.iter().map(|(k, v)| format!("{k}={v:?}")));
        let macros = source
            .host_macros
            .as_ref()
            .map(|exports| {
                exports
                    .iter()
                    .map(|export| {
                        Ok(Macro {
                            name: export.name.to_string(),
                            kind: match export.kind {
                                ProcMacroKind::Function => "function",
                                ProcMacroKind::Attribute => "attribute",
                                ProcMacroKind::Derive => "derive",
                            }
                            .into(),
                            helpers: export
                                .helpers
                                .as_ref()
                                .map(|h| h.iter().map(ToString::to_string).collect()),
                            descriptor: export
                                .expander
                                .flib_descriptor()
                                .ok_or("procedural macro host has no persistent descriptor")?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;
        Ok(Self {
            file: source.file.clone(),
            name: source.name.clone(),
            edition: match source.edition {
                flux_syntax::Edition::E2015 => "2015",
                flux_syntax::Edition::E2018 => "2018",
                flux_syntax::Edition::E2021 => "2021",
                flux_syntax::Edition::E2024 => "2024",
            }
            .into(),
            directory: source.compilation_dir.clone(),
            package: source.package_identity.clone(),
            cfg,
            environment: source.environment.clone(),
            macros,
            externs: Vec::new(),
            output: PathBuf::new(),
            library: true,
            metadata_only: false,
            backend_jobs: 1,
            optimize: options.optimize,
            fast_build: options.fast_build,
            optimize_build_script: options.optimize_build_script,
            debug_assertions: options.debug_assertions(),
            panic_strategy: options.panic_strategy.as_str().into(),
            timings: options.timings,
            native: Default::default(),
        })
    }

    fn source(&self) -> Result<SourceInput, String> {
        let resolver = crate::proc_macro::FlibHostResolver::default();
        let macros = self
            .macros
            .as_ref()
            .map(|exports| {
                exports
                    .iter()
                    .map(|export| {
                        Ok(ProcMacro {
                            name: Symbol::intern(&export.name),
                            kind: match export.kind.as_str() {
                                "function" => ProcMacroKind::Function,
                                "attribute" => ProcMacroKind::Attribute,
                                "derive" => ProcMacroKind::Derive,
                                _ => {
                                    return Err(
                                        "invalid procedural macro kind in build unit".into()
                                    );
                                }
                            },
                            helpers: export
                                .helpers
                                .as_ref()
                                .map(|h| h.iter().map(|s| Symbol::intern(s)).collect()),
                            expander: resolver.resolve(&export.descriptor)?,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()
            })
            .transpose()?;
        Ok(SourceInput {
            file: self.file.clone(),
            name: self.name.clone(),
            edition: crate::cargo::edition(&self.edition)?,
            compilation_dir: self.directory.clone(),
            package_identity: self.package.clone(),
            cfg: crate::cargo::cfg(&self.cfg, &[])?,
            environment: self.environment.clone(),
            host_macros: macros,
            dependencies: Vec::new(),
        })
    }
}

/// Private version-coupled worker protocol, invoked only by the native build driver.
pub(crate) fn worker(path: &Path) -> Result<bool, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let request: Request = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    compile_request(
        &request,
        &Default::default(),
        None,
        request.backend_jobs,
        None,
    )
}

fn compile_request(
    request: &Request,
    artifacts: &FreshArtifacts,
    dependency_files: Option<&[&Path]>,
    workers: usize,
    specializations: Option<Specializations>,
) -> Result<bool, String> {
    let options = BuildOptions {
        panic_strategy: crate::cargo::parse_panic_strategy(&request.panic_strategy)?,
        optimize: request.optimize,
        fast_build: request.fast_build,
        optimize_build_script: request.optimize_build_script,
        debug_assertions: Some(request.debug_assertions),
        timings: request.timings,
    };
    options.validate()?;
    crate::builtins::select_for_strategy(options.panic_strategy)?;
    let _strategy = crate::builtins::strategy_scope(options.panic_strategy)?;
    let source = request.source()?;
    let native = request.native.link();
    let libraries = ExternalLibraries {
        externs: &request.externs,
        dependency_files,
        cache: Some(artifacts),
    };
    let fresh = if request.library {
        crate::flib_build::is_fresh_with_capability(
            &source,
            &request.output,
            options,
            libraries,
            request.metadata_only,
            artifacts,
        )?
    } else {
        crate::flib_build::is_binary_fresh(
            &source,
            &request.output,
            options,
            libraries,
            &native,
            artifacts,
        )?
    };
    if fresh {
        eprintln!("Fresh {}", request.name);
        return Ok(true);
    }
    eprintln!("Compiling {}", request.name);
    crate::backend_workers::with_limit(workers, || {
        crate::flib_build::compile_with_specializations(
            &source,
            ArtifactOutput {
                path: &request.output,
                specializations,
            },
            options,
            if request.library {
                CrateType::Lib
            } else {
                CrateType::Bin
            },
            request.metadata_only,
            libraries,
            &native,
        )
    })
}

pub(crate) fn run(
    sources: &[SourceInput],
    root: usize,
    output: &Path,
    options: BuildOptions,
    jobs: usize,
    library: bool,
    native: &flux_codegen::NativeLink,
) -> Result<ExitCode, String> {
    let parent = output.parent().ok_or("build output has no parent")?;
    let artifacts = FreshArtifacts::for_outputs(parent);
    run_in_directory(
        sources,
        root,
        Destination {
            output,
            dependencies: &parent.join("deps"),
            artifacts: &artifacts,
            shared_outputs: false,
            optimize_root: false,
        },
        options,
        jobs,
        library,
        native,
    )
}

/// All callers preparing scripts for the same plan use the same dependency directory.
pub(super) fn run_script(
    sources: &[SourceInput],
    output: &Path,
    directory: &Path,
    artifacts: &FreshArtifacts,
    options: BuildOptions,
    jobs: usize,
    native: &flux_codegen::NativeLink,
) -> Result<ExitCode, String> {
    let root = sources.len().checked_sub(1).ok_or("missing build script")?;
    run_in_directory(
        sources,
        root,
        Destination {
            output,
            dependencies: directory,
            artifacts,
            shared_outputs: true,
            optimize_root: options.optimize_build_script,
        },
        BuildOptions {
            optimize_build_script: false,
            ..options
        },
        jobs,
        false,
        native,
    )
}

struct Destination<'a> {
    output: &'a Path,
    dependencies: &'a Path,
    artifacts: &'a FreshArtifacts,
    shared_outputs: bool,
    optimize_root: bool,
}

fn run_in_directory(
    sources: &[SourceInput],
    root: usize,
    Destination {
        output,
        dependencies: directory,
        artifacts,
        shared_outputs,
        optimize_root,
    }: Destination<'_>,
    options: BuildOptions,
    jobs: usize,
    library: bool,
    native: &flux_codegen::NativeLink,
) -> Result<ExitCode, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let needed = needed_units(sources, root)?;
    let mut requests: BTreeMap<usize, (PathBuf, Request)> = BTreeMap::new();
    for &index in &needed {
        let mut request = Request::new(&sources[index], options)?;
        if index == root && !library {
            request.native = native.into();
            request.optimize_build_script |= optimize_root;
            // Every needed dependency is finished before the binary starts.
            request.backend_jobs = jobs;
        }
        // Native-linked executables keep one whole-program backend pass. The
        // DAG caches checked MIR instead of multiplying generic native objects.
        request.metadata_only =
            (index != root) && !library && crate::flib_build::whole_program_native(native);
        request.externs = sources[index]
            .dependencies
            .iter()
            .map(|(name, id)| (name.clone(), requests[id].1.output.clone()))
            .collect();
        // Identity excludes plan position, compiler version, timing output and dependency contents.
        // Configuration and compiler changes are checked by the artifact's input snapshot.
        let identity = serde_json::to_vec(&(
            &request.file,
            &request.name,
            &request.edition,
            &request.package,
            &request.cfg,
            request.metadata_only,
        ))
        .map_err(|e| e.to_string())?;
        // Script subgraphs may build concurrently into the same directory.
        // Do not share a destination between requests that use different
        // generated source directories or dependency configurations. Values
        // produced by a script stay in the freshness snapshot.
        let identity = if shared_outputs {
            let generated_dir = request
                .environment
                .as_ref()
                .and_then(|env| env.get("OUT_DIR"));
            serde_json::to_vec(&(
                identity,
                generated_dir,
                &request.externs,
                (index == root).then_some(output),
            ))
            .map_err(|e| e.to_string())?
        } else {
            identity
        };
        let hash = blake3::hash(&identity).to_hex();
        let stem = format!("{}-{}", request.name, &hash[..16]);
        request.output = if index == root && !library {
            output.to_owned()
        } else {
            directory.join(format!("{stem}.flib"))
        };
        request.library = index != root || library;
        let path = directory.join(format!("{stem}.unit.json"));
        let bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
        let unchanged = std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_file())
            && std::fs::read(&path).is_ok_and(|saved| saved == bytes);
        if !unchanged {
            crate::flib::write_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        }
        requests.insert(index, (path, request));
    }
    let mut specialization_producer = None;
    if !library
        && !shared_outputs
        && !optimize_root
        && !crate::flib_build::whole_program_native(native)
        && crate::flib_build::supports_dependency_specializations(options)
    {
        for (position, (_, index)) in sources[root].dependencies.iter().enumerate() {
            if sources[*index].host_macros.is_none()
                && needed_units(sources, *index)?.len() + 1 == needed.len()
            {
                specialization_producer = Some(*index);
                let externs = &mut requests
                    .get_mut(&root)
                    .expect("requested executable")
                    .1
                    .externs;
                // Resolving this library first gives both jobs the same artifact
                // loading order and type identities.
                let producer = externs.remove(position);
                externs.insert(0, producer);
                break;
            }
        }
    }
    let dependency_files = |index| -> Result<Vec<&Path>, String> {
        Ok(needed_units(sources, index)?
            .into_iter()
            .filter(|&dependency| dependency != index)
            .map(|dependency| requests[&dependency].1.output.as_path())
            .collect())
    };
    let prepare_specializations = |producer: usize| {
        let files = dependency_files(root).ok()?;
        let (_, root_request) = &requests[&root];
        let source = root_request.source().ok()?;
        if crate::flib_build::is_binary_fresh(
            &source,
            &root_request.output,
            options,
            ExternalLibraries {
                externs: &root_request.externs,
                dependency_files: Some(&files),
                cache: Some(artifacts),
            },
            native,
            artifacts,
        )
        .ok()?
        {
            return None;
        }
        crate::backend_workers::with_limit(jobs, || {
            crate::flib_build::prepare_library_specializations(
                &requests[&producer].1.output,
                &files,
                artifacts,
                options,
            )
        })
    };
    let mut prepared_specializations = None;
    let (finished, completions) = std::sync::mpsc::channel();
    let mut started = BTreeSet::new();
    let mut done = BTreeSet::new();
    let mut running = 0;
    let mut reservations = BTreeMap::new();
    let mut failure = None;
    let mut diagnosed = false;
    std::thread::scope(|scope| -> Result<(), String> {
        loop {
            if failure.is_none() && !diagnosed {
                let ready = |index: usize, started: &BTreeSet<usize>| {
                    !started.contains(&index)
                        && sources[index]
                            .dependencies
                            .iter()
                            .all(|(_, id)| done.contains(id))
                };
                for (&index, (_, request)) in &requests {
                    if running == jobs {
                        break;
                    }
                    if !ready(index, &started) {
                        continue;
                    }
                    started.insert(index);
                    // When this library is the only runnable job, its borrow checks
                    // and dependency loading can use slots that would otherwise idle.
                    let sole_ready_library = jobs > 1
                        && !shared_outputs
                        && request.library
                        && running == 0
                        && !requests.keys().any(|&other| ready(other, &started));
                    let workers = if sole_ready_library {
                        jobs
                    } else {
                        request.backend_jobs
                    };
                    running += workers;
                    reservations.insert(index, workers);
                    let specializations = if index == root {
                        prepared_specializations.take()
                    } else {
                        None
                    };
                    let prepare_specializations = &prepare_specializations;
                    let finished = finished.clone();
                    // Compilation used to run on a process's main thread.
                    // Recursive parsing and evaluation need more than the
                    // default 2 MiB Rust worker stack.
                    std::thread::Builder::new()
                        .stack_size(8 * 1024 * 1024)
                        .spawn_scoped(scope, move || {
                            let mut completion = JobCompletion {
                                sender: finished,
                                index,
                                result: None,
                            };
                            // Each job owns its source/dependency state and creates
                            // solver scopes on this thread. Only immutable builtins
                            // and the synchronized interners are shared across jobs.
                            completion.result = Some(dependency_files(index).and_then(|files| {
                                let lock =
                                    shared_outputs.then(|| artifacts.output_lock(&request.output));
                                let _guard = lock.as_ref().map(|lock| {
                                    lock.lock().expect("shared host output lock poisoned")
                                });
                                let success = compile_request(
                                    request,
                                    artifacts,
                                    Some(&files),
                                    workers,
                                    specializations,
                                )?;
                                let prepared = (success && specialization_producer == Some(index))
                                    .then(|| prepare_specializations(index))
                                    .flatten();
                                Ok((success, prepared))
                            }));
                        })
                        .map_err(|error| format!("cannot start compiler unit: {error}"))?;
                }
            }
            if running == 0 {
                break;
            }
            let (index, result) = completions.recv().map_err(|e| e.to_string())?;
            running -= reservations
                .remove(&index)
                .expect("finished job has a reservation");
            match result {
                Ok((true, specializations)) => {
                    if specializations.is_some() {
                        prepared_specializations = specializations;
                    }
                    done.insert(index);
                }
                Ok((false, _)) => {
                    eprintln!("compilation of {} failed", sources[index].name);
                    diagnosed = true;
                }
                Err(error) => {
                    failure.get_or_insert(error);
                }
            }
        }
        Ok(())
    })?;
    if let Some(error) = failure {
        return Err(error);
    }
    if diagnosed {
        return Ok(ExitCode::FAILURE);
    }
    if done.len() != needed.len() {
        return Err("build units depend on each other in a cycle".into());
    }
    if library {
        let request = &requests[&root].1;
        let files = dependency_files(root)?;
        if !crate::flib_build::is_fresh_with_capability(
            &sources[root],
            output,
            options,
            ExternalLibraries {
                externs: &request.externs,
                dependency_files: Some(&files),
                cache: Some(artifacts),
            },
            false,
            artifacts,
        )? {
            let bytes = std::fs::read(&request.output).map_err(|e| e.to_string())?;
            crate::flib::write_atomic(output, &bytes).map_err(|e| e.to_string())?;
        }
        if !shared_outputs && crate::flib_build::supports_dependency_specializations(options) {
            crate::backend_workers::with_limit(jobs, || {
                crate::flib_build::prepare_library_specializations(
                    &request.output,
                    &files,
                    artifacts,
                    options,
                )
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn needed_units(sources: &[SourceInput], root: usize) -> Result<BTreeSet<usize>, String> {
    let mut needed = BTreeSet::new();
    let mut pending = vec![root];
    while let Some(index) = pending.pop() {
        if !needed.insert(index) {
            continue;
        }
        let source = sources.get(index).ok_or("missing build unit")?;
        for (_, dependency) in &source.dependencies {
            if *dependency >= index {
                return Err("build units are not in dependency order".into());
            }
            pending.push(*dependency);
        }
    }
    Ok(needed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panicking_job_notifies_scheduler() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _completion = JobCompletion::<bool> {
                sender,
                index: 7,
                result: None,
            };
            panic!("test compiler job panic");
        });
        let (index, result) = receiver
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert_eq!(index, 7);
        assert_eq!(result.unwrap_err(), "compiler unit panicked");
        assert!(worker.join().is_err());
    }

    #[test]
    fn completed_job_reports_its_result_once() {
        let (sender, receiver) = std::sync::mpsc::channel();
        drop(JobCompletion {
            sender,
            index: 3,
            result: Some(Ok(true)),
        });
        assert_eq!(receiver.recv().unwrap(), (3, Ok(true)));
        assert!(receiver.recv().is_err());
    }
}
