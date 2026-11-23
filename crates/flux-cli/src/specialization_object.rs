//! Reuses the native requirements of separately compiled dependencies.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use flux_codegen::{Compilation, ObjectOptions, OutputKind, Target};
use flux_hir::{Hir, ItemTree};
use flux_mir::imported::ImportedMir;
use object::{Object, ObjectSymbol};

use crate::build::Timings;
use crate::flib;

fn cache_path(output: &Path) -> PathBuf {
    let mut path = output.as_os_str().to_owned();
    path.push(".dependency-specializations");
    PathBuf::from(path)
}

pub(super) struct Cache {
    key: [u8; 32],
    object: Option<Specializations>,
}

impl Cache {
    pub(super) fn open(
        output: &Path,
        checksums: &[u8],
        prepared: Option<Specializations>,
    ) -> Option<Self> {
        if checksums.is_empty() {
            return None;
        }
        let mut hash = blake3::Hasher::new();
        hash.update(b"flux-dependency-specializations-darwin-arm64-release-abort-v1");
        hash.update(&flib::compiler_identity().ok()?);
        let builtins = crate::builtins::object_hash(true);
        hash.update(&[u8::from(builtins.is_some())]);
        if let Some(object_hash) = builtins {
            hash.update(object_hash);
        }
        hash.update(checksums);
        let key = *hash.finalize().as_bytes();
        let object = prepared
            .filter(|object| object.entry[..32] == key)
            .or_else(|| {
                std::fs::read(cache_path(output))
                    .ok()
                    .filter(|entry| {
                        entry.get(..32) == Some(&key)
                            && entry.get(64..).is_some_and(|object| {
                                entry[32..64] == *blake3::hash(object).as_bytes()
                            })
                    })
                    .and_then(|entry| Specializations::new(entry, false))
            });
        Some(Self { key, object })
    }

    pub(super) fn into_object(self) -> Option<Specializations> {
        self.object
    }

    pub(super) fn needs_build(&self) -> bool {
        self.object.is_none()
    }

    pub(super) fn prepare(
        self,
        dependencies: Option<(ItemTree, &flux_ty::flib::LoadedTypes, &mut ImportedMir)>,
        native: &HashSet<String>,
        compilation: Compilation<'_>,
        policy: (Target, ObjectOptions),
        timings: &mut Timings,
    ) -> Option<Specializations> {
        if let Some(object) = self.object {
            return Some(object);
        }
        let (items, loaded, imported) = dependencies?;
        let hir = Hir::from_checked_items(items);
        let mut sigs = loaded.signatures().clone();
        sigs.type_ids = sigs.type_ids.fork();
        sigs.type_ids.set_crate_hashes(compilation.crate_hashes);
        let typed = flux_ty::TypedFile {
            sigs,
            bodies: Default::default(),
            consts: Default::default(),
            diagnostics: Vec::new(),
            warnings: Vec::new(),
        };
        let mut module = timings.time("dependency specialization lowering", || {
            imported.with_codegen_inputs(|mir, required_instances| {
                if required_instances.is_empty() {
                    return None;
                }
                flux_ty::Ty::with_substitution_cache(|| {
                    flux_codegen::lower_linked_executable(
                        &hir,
                        &typed,
                        mir,
                        Compilation {
                            kind: OutputKind::DependencySpecializations,
                            required_functions: &[],
                            required_instances,
                            ..compilation
                        },
                        native,
                        None,
                    )
                    .ok()
                    .map(|lowered| lowered.module)
                })
            })
        })?;
        let bytes = timings
            .time("dependency specialization backend", || {
                flux_codegen::compile_object_with_options(&mut module, policy.0, policy.1)
            })
            .ok()?;
        let mut entry = Vec::with_capacity(64 + bytes.len());
        entry.extend_from_slice(&self.key);
        entry.extend_from_slice(blake3::hash(&bytes).as_bytes());
        entry.extend_from_slice(&bytes);
        Specializations::new(entry, true)
    }
}

pub(crate) struct Specializations {
    entry: Vec<u8>,
    publish: bool,
    pub(super) symbols: HashSet<String>,
}

impl Specializations {
    fn new(entry: Vec<u8>, publish: bool) -> Option<Self> {
        let object = object::File::parse(entry.get(64..)?).ok()?;
        if object.format() != object::BinaryFormat::MachO
            || object.architecture() != object::Architecture::Aarch64
            || object.kind() != object::ObjectKind::Relocatable
        {
            return None;
        }
        let symbols: HashSet<_> = object
            .symbols()
            .filter(|symbol| {
                symbol.is_definition()
                    && !symbol.is_local()
                    && symbol.kind() == object::SymbolKind::Text
            })
            .filter_map(|symbol| symbol.name().ok()?.strip_prefix('_').map(str::to_owned))
            .collect();
        (!symbols.is_empty()).then_some(Self {
            entry,
            publish,
            symbols,
        })
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.entry[64..]
    }

    pub(super) fn publish(&mut self, output: &Path, timings: &mut Timings) {
        if self.publish {
            timings.time("dependency specialization cache write", || {
                // A linked executable remains usable if its disposable cache
                // cannot be written.
                let _ = flib::write_cache(&cache_path(output), &self.entry);
            });
            self.publish = false;
        }
    }
}
