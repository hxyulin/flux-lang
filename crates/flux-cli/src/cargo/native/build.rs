//! Dependency-ordered Fluxlib builds sharing immutable compiler-owned libraries.
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use flux_hir::{ProcMacro, ProcMacroKind, ProcMacroResolver};
use flux_intern::Symbol;
use serde::{Deserialize, Serialize};

use crate::build::SourceInput;
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
struct JobCompletion {
    sender: std::sync::mpsc::Sender<(usize, Result<bool, String>)>,
    index: usize,
    result: Option<Result<bool, String>>,
}

impl Drop for JobCompletion {
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
    debug_assertions: bool,
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
            debug_assertions: options.debug_assertions(),
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
    let source = request.source()?;
    let options = BuildOptions {
        panic_strategy: flux_syntax::PanicStrategy::Abort,
        optimize: request.optimize,
        fast_build: request.fast_build,
        debug_assertions: Some(request.debug_assertions),
        timings: request.timings,
    };
    let native = request.native.link();
    let fresh = if request.library {
        crate::flib_build::is_fresh_with_capability(
            &source,
            &request.output,
            options,
            &request.externs,
            request.metadata_only,
        )?
    } else {
        crate::flib_build::is_binary_fresh(
            &source,
            &request.output,
            options,
            &request.externs,
            &native,
        )?
    };
    if fresh {
        eprintln!("Fresh {}", request.name);
        return Ok(true);
    }
    eprintln!("Compiling {}", request.name);
    crate::backend_workers::with_limit(request.backend_jobs, || {
        crate::flib_build::compile_native(
            &source,
            &request.output,
            options,
            if request.library {
                CrateType::Lib
            } else {
                CrateType::Bin
            },
            request.metadata_only,
            &request.externs,
            &native,
        )
    })
}

pub(super) fn run(
    sources: &[SourceInput],
    root: usize,
    output: &Path,
    options: BuildOptions,
    jobs: usize,
    library: bool,
    native: &flux_codegen::NativeLink,
) -> Result<ExitCode, String> {
    let parent = output.parent().ok_or("build output has no parent")?;
    let directory = parent.join("deps");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
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
    let mut requests: BTreeMap<usize, (PathBuf, Request)> = BTreeMap::new();
    for &index in &needed {
        let mut request = Request::new(&sources[index], options)?;
        if index == root && !library {
            request.native = native.into();
            // Every needed dependency is finished before the binary starts.
            request.backend_jobs = jobs;
        }
        // Native-linked executables keep one whole-program backend pass. The
        // DAG caches checked MIR instead of multiplying generic native objects.
        request.metadata_only = (index != root) && !library && *native != Default::default();
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
        let hash = blake3::hash(&identity).to_hex();
        let stem = format!("{}-{}", request.name, &hash[..16]);
        request.output = if index == root && !library {
            output.to_owned()
        } else {
            directory.join(format!("{stem}.flib"))
        };
        request.library = index != root || library;
        request.externs = sources[index]
            .dependencies
            .iter()
            .map(|(name, id)| (name.clone(), requests[id].1.output.clone()))
            .collect();
        let path = directory.join(format!("{stem}.unit.json"));
        crate::flib::write_atomic(
            &path,
            &serde_json::to_vec(&request).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        requests.insert(index, (path, request));
    }
    let (finished, completions) = std::sync::mpsc::channel();
    let mut started = BTreeSet::new();
    let mut done = BTreeSet::new();
    let mut running = 0;
    let mut failure = None;
    std::thread::scope(|scope| -> Result<(), String> {
        loop {
            if failure.is_none() {
                for (&index, (path, _)) in &requests {
                    if running == jobs {
                        break;
                    }
                    if started.contains(&index)
                        || !sources[index]
                            .dependencies
                            .iter()
                            .all(|(_, id)| done.contains(id))
                    {
                        continue;
                    }
                    started.insert(index);
                    running += 1;
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
                            completion.result = Some(worker(path));
                        })
                        .map_err(|error| format!("cannot start compiler unit: {error}"))?;
                }
            }
            if running == 0 {
                break;
            }
            let (index, result) = completions.recv().map_err(|e| e.to_string())?;
            running -= 1;
            match result {
                Ok(true) => {
                    done.insert(index);
                }
                Ok(false) => {
                    failure.get_or_insert(format!("compilation of {} failed", sources[index].name));
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
    if done.len() != needed.len() {
        return Err("build units depend on each other in a cycle".into());
    }
    if library {
        let request = &requests[&root].1;
        if !crate::flib_build::is_fresh(&sources[root], output, options, &request.externs)? {
            let bytes = std::fs::read(&request.output).map_err(|e| e.to_string())?;
            crate::flib::write_atomic(output, &bytes).map_err(|e| e.to_string())?;
        }
    }
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panicking_job_notifies_scheduler() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _completion = JobCompletion {
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
