//! Experimental fast-build partitioning under the native graph's worker budget.

use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

/// The experiment is explicit and participates in executable freshness.
pub(crate) fn count(enabled: bool) -> Result<NonZeroUsize, String> {
    let count = if enabled {
        match std::env::var_os("FLUX_BACKEND_UNITS") {
            None => 1,
            Some(value) => value
                .to_str()
                .and_then(|value| value.parse::<usize>().ok())
                .filter(|&count| (1..=256).contains(&count))
                .ok_or("FLUX_BACKEND_UNITS must be between 1 and 256")?,
        }
    } else {
        1
    };
    Ok(NonZeroUsize::new(count).expect("unit count is positive"))
}

pub(crate) fn compile(
    module: flux_codegen::Module,
    count: NonZeroUsize,
    target: flux_codegen::Target,
    output: &Path,
    panic_strategy: flux_syntax::PanicStrategy,
) -> Result<Vec<Vec<u8>>, String> {
    let modules = flux_codegen::split_fast_module(module, count).map_err(|e| e.to_string())?;
    let units = modules.len();
    let jobs = crate::backend_workers::limit().get().min(units).max(1);
    let cache = if std::env::var_os("FLUX_BACKEND_CACHE").as_deref()
        == Some(std::ffi::OsStr::new("1"))
        && std::env::var_os("FLUX_KEEL_PROFILE").is_none()
    {
        let directory = output.with_extension("backend-units");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        Some((directory, crate::flib::compiler_identity()?))
    } else {
        None
    };
    eprintln!("backend units: {units}, workers: {jobs}");
    let mut pending: Vec<_> = modules.into_iter().enumerate().collect();
    pending.reverse();
    let pending = Mutex::new(pending);
    let results = Mutex::new(Vec::with_capacity(units));
    std::thread::scope(|scope| -> Result<(), String> {
        let mut workers = Vec::new();
        for _ in 0..jobs {
            let pending = &pending;
            let results = &results;
            let cache = &cache;
            workers.push(
                std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024)
                    .spawn_scoped(scope, move || {
                        loop {
                            let next = pending.lock().expect("backend queue lock poisoned").pop();
                            let Some((index, mut module)) = next else {
                                break;
                            };
                            let entry = cache
                                .as_ref()
                                .filter(|_| {
                                    module.assembly.is_empty()
                                        && module.files.is_empty()
                                        && module.debug_types.is_empty()
                                        && module.debug_globals.is_empty()
                                        && module.debug_language.is_none()
                                })
                                .map(|(directory, compiler)| {
                                    Entry::new(
                                        directory,
                                        index,
                                        compiler,
                                        target,
                                        &module,
                                        panic_strategy,
                                    )
                                });
                            let cached = entry.as_ref().and_then(Entry::read);
                            let hit = cached.is_some();
                            let result = if let Some(bytes) = cached {
                                Ok(bytes)
                            } else {
                                flux_codegen::compile_object_with_options(
                                    &mut module,
                                    target,
                                    flux_codegen::ObjectOptions {
                                        optimization: flux_codegen::ObjectOptimization::FastBuild,
                                        panic_strategy,
                                    },
                                )
                            };
                            if !hit && let (Some(entry), Ok(bytes)) = (&entry, &result) {
                                entry.record(bytes);
                            }
                            results
                                .lock()
                                .expect("backend results lock poisoned")
                                .push((index, result, hit));
                        }
                    })
                    .map_err(|e| format!("cannot start backend worker: {e}"))?,
            );
        }
        for worker in workers {
            worker.join().expect("backend worker panicked");
        }
        Ok(())
    })?;
    let mut results = results.into_inner().expect("backend results lock poisoned");
    results.sort_unstable_by_key(|(index, _, _)| *index);
    if cache.is_some() {
        let hits = results.iter().filter(|(_, _, hit)| *hit).count();
        eprintln!("backend units reused: {hits}/{units}");
    }
    results
        .into_iter()
        .map(|(_, result, _)| result.map_err(|e| e.to_string()))
        .collect()
}

#[derive(Serialize, Deserialize)]
struct Record {
    key: [u8; 32],
    object: [u8; 32],
}

struct Entry {
    metadata: PathBuf,
    object: PathBuf,
    key: [u8; 32],
}

impl Entry {
    fn new(
        directory: &Path,
        index: usize,
        compiler: &[u8; 32],
        target: flux_codegen::Target,
        module: &flux_codegen::Module,
        panic_strategy: flux_syntax::PanicStrategy,
    ) -> Self {
        struct HashText(blake3::Hasher);
        impl std::fmt::Write for HashText {
            fn write_str(&mut self, text: &str) -> std::fmt::Result {
                self.0.update(text.as_bytes());
                Ok(())
            }
        }
        use std::fmt::Write;
        let mut hash = HashText(blake3::Hasher::new());
        hash.0.update(b"flux-fast-unit-v1");
        hash.0.update(compiler);
        hash.0.update(target.triple().as_bytes());
        hash.0.update(panic_strategy.as_str().as_bytes());
        write!(&mut hash, "{module}").expect("hash text writes cannot fail");
        // Target admission checks inspect unused indirect signatures too.
        // Module's text format prints only signatures referenced by the body.
        for function in &module.functions {
            write!(
                &mut hash,
                "function-signatures {}:{}",
                function.name.len(),
                function.name
            )
            .expect("hash text writes cannot fail");
            for (_, signature) in function.signatures() {
                writeln!(&mut hash, "{signature:?}").expect("hash text writes cannot fail");
            }
        }
        Self {
            metadata: directory.join(format!("unit-{index}.json")),
            object: directory.join(format!("unit-{index}.o")),
            key: *hash.0.finalize().as_bytes(),
        }
    }

    fn read(&self) -> Option<Vec<u8>> {
        let record: Record = serde_json::from_slice(&std::fs::read(&self.metadata).ok()?).ok()?;
        if record.key != self.key {
            return None;
        }
        let bytes = std::fs::read(&self.object).ok()?;
        (blake3::hash(&bytes).as_bytes() == &record.object).then_some(bytes)
    }

    fn record(&self, bytes: &[u8]) {
        let record = Record {
            key: self.key,
            object: *blake3::hash(bytes).as_bytes(),
        };
        let result = crate::flib::write_atomic(&self.object, bytes).and_then(|()| {
            let metadata =
                serde_json::to_vec(&record).expect("unit record contains only byte arrays");
            crate::flib::write_atomic(&self.metadata, &metadata)
        });
        if let Err(error) = result {
            eprintln!("warning: cannot cache backend unit: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unprinted_signatures_participate_in_cache_identity() {
        let (_, _, hir) = flux_hir::lower_text("cache-key.rs", "fn main() {}");
        let typed = flux_mir::check_file(&hir);
        assert!(typed.diagnostics.is_empty());
        let mir = flux_mir::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty());
        let mut module = flux_codegen::lower_module(&hir, &typed, &mir).unwrap();
        let target = flux_codegen::Target::X86_64LinuxGnu;
        let before = Entry::new(
            Path::new("cache"),
            0,
            &[0; 32],
            target,
            &module,
            flux_syntax::PanicStrategy::Abort,
        )
        .key;
        let printed = module.to_string();
        let mut signature = module.functions[0].sig.clone();
        signature.variadic = true;
        module.functions[0].import_signature(signature);
        assert_eq!(module.to_string(), printed);
        let after = Entry::new(
            Path::new("cache"),
            0,
            &[0; 32],
            target,
            &module,
            flux_syntax::PanicStrategy::Abort,
        )
        .key;
        assert_ne!(before, after);
    }
}
