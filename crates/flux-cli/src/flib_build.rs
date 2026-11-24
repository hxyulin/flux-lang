//! Driver for explicit Fluxlib compilation.

use crate::build::SourceInput;
use crate::flib::freshness::{MacroFreshness, Snapshot};
use crate::flib::semantic::Capability;
use crate::flib::semantic::{
    self, CheckedCrate, DependencyState, crate_hash_with_indices, write_checked_with_indices,
};
use crate::{BuildOptions, CrateType, report_all};
use flux_hir::flib::{Encode, Encoder};
use std::collections::HashSet;
use std::path::Path;

#[path = "specialization_object.rs"]
pub(crate) mod specialization_object;

pub(crate) fn library(
    file: &Path,
    output: &Path,
    name: &str,
    options: BuildOptions,
    metadata_only: bool,
    externs: &[(String, std::path::PathBuf)],
) -> Result<bool, String> {
    compile(
        &SourceInput::standalone(file, name),
        output,
        options,
        CrateType::Lib,
        metadata_only,
        externs,
    )
}

/// Compiles one source crate against a closed set of checked Flux libraries.
pub(crate) fn compile(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    kind: CrateType,
    metadata_only: bool,
    externs: &[(String, std::path::PathBuf)],
) -> Result<bool, String> {
    compile_native(
        input,
        output,
        options,
        kind,
        metadata_only,
        ExternalLibraries {
            externs,
            dependency_files: None,
            cache: None,
        },
        &Default::default(),
    )
}

/// Direct import aliases and, for a package build, its complete artifact paths.
#[derive(Clone, Copy)]
pub(crate) struct ExternalLibraries<'a> {
    pub externs: &'a [(String, std::path::PathBuf)],
    pub dependency_files: Option<&'a [&'a Path]>,
    pub cache: Option<&'a semantic::FreshArtifacts>,
}

impl ExternalLibraries<'_> {
    fn macro_freshness(self) -> MacroFreshness {
        if self.dependency_files.is_some() {
            MacroFreshness::Cargo
        } else {
            MacroFreshness::Reexpand
        }
    }
}

pub(crate) fn compile_native(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    kind: CrateType,
    metadata_only: bool,
    libraries: ExternalLibraries<'_>,
    native: &flux_codegen::NativeLink,
) -> Result<bool, String> {
    compile_native_pipelined(
        input,
        ArtifactOutput {
            path: output,
            pipeline: None,
            specializations: None,
        },
        options,
        kind,
        metadata_only,
        libraries,
        native,
    )
}

pub(crate) type PublishMetadata<'a> = dyn Fn(&[u8]) -> Result<(), String> + 'a;

pub(crate) struct ArtifactOutput<'a> {
    pub path: &'a Path,
    pub pipeline: Option<PipelinedLibrary<'a>>,
    pub specializations: Option<specialization_object::Specializations>,
}

/// The build owns this staging directory and publishes its archives only
/// after the remaining native compilation succeeds.
pub(crate) struct PipelinedLibrary<'a> {
    pub directory: &'a Path,
    pub output: &'a Path,
    pub publish: Option<&'a PublishMetadata<'a>>,
}

pub(crate) fn compile_native_pipelined(
    input: &SourceInput,
    ArtifactOutput {
        path: output,
        pipeline,
        specializations,
    }: ArtifactOutput<'_>,
    options: BuildOptions,
    kind: CrateType,
    metadata_only: bool,
    libraries: ExternalLibraries<'_>,
    native: &flux_codegen::NativeLink,
) -> Result<bool, String> {
    debug_assert!(pipeline.is_none() || (kind == CrateType::Lib && !metadata_only));
    options.validate()?;
    let _strategy = crate::builtins::strategy_scope(options.panic_strategy)?;
    let whole_program = kind == CrateType::Bin && whole_program_native(native);
    let link_dependency_objects = kind == CrateType::Bin && !whole_program;
    let capability = if metadata_only {
        Capability::Metadata
    } else {
        Capability::Object
    };
    let externs = libraries.externs;
    let paths: Vec<_> = externs.iter().map(|(_, path)| path.clone()).collect();
    // Standalone inputs keep eager verification. Managed graphs only defer
    // function records from complete, separately compiled dependency objects.
    let defer_functions = !cfg!(debug_assertions)
        && !whole_program
        && libraries.dependency_files.is_some()
        && libraries.cache.is_some();
    use semantic::{ArchiveRead, Recheck};
    let archive_read = if kind == CrateType::Lib && defer_functions {
        ArchiveRead::Metadata
    } else {
        ArchiveRead::Full
    };
    let workers = crate::backend_workers::limit().get();
    let overlap =
        defer_functions && !paths.is_empty() && workers > 1 && crate::builtins::needs_activation();
    let dependency_phase = if overlap {
        "built-ins and dependency artifact load"
    } else {
        crate::builtins::activate_selected()?;
        "dependency artifact load"
    };
    let units = crate::backend_units::count(options.fast_build && kind == CrateType::Bin)?;
    let cache_specializations = cache_specializations(options, kind, libraries)
        && !metadata_only
        && link_dependency_objects
        && units.get() == 1
        && native_cutoff_enabled();
    let mut timings = crate::build::Timings::new(options.timings, "compile scoped elapsed");
    let (
        mut state,
        deferred,
        mut checked_dependencies,
        all_linkable,
        recheck,
        specialization_checksums,
        demand_inputs,
    ) = timings.time(dependency_phase, || {
        let load = |state: DependencyState, dependencies: semantic::DependencyInputs| {
            let all_linkable =
                dependencies.all_linkable(pipeline.as_ref().map(|stage| stage.directory));
            let mut checked_dependencies = Vec::new();
            if kind == CrateType::Lib
                && libraries.dependency_files.is_some()
                && input.host_macros.is_none()
                && let Ok(root) = std::fs::read(&input.file)
            {
                checked_dependencies = dependencies.checksums();
                if !checked_dependencies.is_empty() {
                    checked_dependencies.extend_from_slice(&crate::flib::digest(&root));
                }
            }
            let recheck = if checked_dependencies.is_empty() {
                Recheck::Full
            } else {
                semantic::reusable_library(output, &checked_dependencies, libraries.cache)
                    .ok()
                    .filter(|(manifest, _)| metadata_only || manifest.linkable)
                    .map_or(Recheck::Full, |(_, recheck)| recheck)
            };
            let specialization_checksums =
                (cache_specializations && all_linkable).then(|| dependencies.checksums());
            let defer_ir = recheck != Recheck::Full;
            let state = state.load_inputs(
                &dependencies,
                defer_ir,
                defer_functions,
                link_dependency_objects,
            )?;
            let (deferred, demand_inputs) = if defer_ir {
                (Some(dependencies), None)
            } else {
                let demand =
                    specialization_checksums.is_some() && large_specialization_graph(&state);
                (None, demand.then_some(dependencies))
            };
            Ok::<_, String>((
                state,
                deferred,
                checked_dependencies,
                all_linkable,
                recheck,
                specialization_checksums,
                demand_inputs,
            ))
        };
        std::thread::scope(|scope| {
            // Discovery only needs identities, which are known before
            // decoding the compiler-owned prefix.
            let mut pending = if overlap {
                let known = semantic::embedded_hashes()?;
                let paths = &paths;
                let files = libraries.dependency_files;
                let cache = libraries.cache;
                std::thread::Builder::new()
                    .spawn_scoped(scope, move || {
                        crate::backend_workers::with_limit(workers - 1, || {
                            semantic::DependencyInputs::resolve(
                                paths,
                                &known,
                                files,
                                cache,
                                archive_read,
                            )
                        })
                    })
                    .ok()
            } else {
                None
            };
            let mut resolve = |state: &DependencyState| match pending.take() {
                Some(worker) => worker.join().expect("dependency discovery panicked"),
                None => state.resolve(
                    &paths,
                    libraries.dependency_files,
                    libraries.cache,
                    archive_read,
                ),
            };
            if overlap {
                let mut loaded = None;
                crate::builtins::activate_selected_while(&mut |db, items, sigs| {
                    loaded = Some(crate::backend_workers::with_limit(workers - 1, || {
                        let state = crate::flib::time("dependency state initialization", || {
                            DependencyState::from_interface(db, items, sigs)
                        })?;
                        let dependencies = resolve(&state)?;
                        load(state, dependencies)
                    }));
                })?;
                if let Some(loaded) = loaded {
                    return loaded;
                }
            }
            let state = crate::flib::time("dependency state initialization", DependencyState::new)?;
            let dependencies = resolve(&state)?;
            load(state, dependencies)
        })
    })?;
    let mut aliases = Vec::new();
    let mut imports = Vec::new();
    for ((alias, _), &hash) in externs.iter().zip(&state.roots) {
        if imports
            .iter()
            .any(|(name, _)| name == &flux_intern::Symbol::intern(alias))
        {
            return Err(format!("duplicate external crate alias `{alias}`"));
        }
        if matches!(
            alias.as_str(),
            "core" | "alloc" | "std" | "self" | "super" | "crate"
        ) {
            return Err(format!("external crate alias `{alias}` is reserved"));
        }
        let dependency = state
            .dependencies
            .iter()
            .find(|d| d.hash == hash)
            .ok_or("missing loaded dependency")?;
        imports.push((flux_intern::Symbol::intern(alias), dependency.krate));
        aliases.push((alias.clone(), hash));
    }
    let builtin_count = state
        .items
        .crates
        .iter()
        .filter(|c| c.root.into_raw() < state.items.core.modules)
        .count();
    let loaded_hashes: Vec<_> = state
        .dependencies
        .iter()
        .filter(|d| d.krate.0 as usize >= builtin_count)
        .map(|d| (d.krate, d.hash))
        .collect();
    if capability == Capability::Object && !whole_program && !all_linkable {
        return Err("a dependency Fluxlib has no native object".into());
    }
    let specialization_cache = specialization_checksums.as_deref().and_then(|checksums| {
        timings.time("dependency specialization cache read", || {
            specialization_object::Cache::open(output, checksums, specializations)
        })
    });
    let mut demand_cache = demand_inputs.as_ref().and_then(|_| {
        specialization_cache
            .as_ref()
            .filter(|cache| cache.needs_build())?;
        specialization_object::Cache::open_demand(output, specialization_checksums.as_deref()?)
    });
    let dependency_items = (demand_cache.is_none()
        && specialization_cache
            .as_ref()
            .is_some_and(specialization_object::Cache::needs_build))
    .then(|| state.items.clone());
    let semantic_inputs = if deferred.is_some() {
        None
    } else {
        Some(prepare_semantics(&mut state)?)
    };
    let dependency_counts = state.items.counts();
    let root = if let Some(exports) = &input.host_macros {
        state
            .db
            .add_proc_macro_crate(input.file.clone(), exports.clone())
    } else {
        state.db.load_with_edition(&input.file, input.edition)?
    };
    state.db.set_debug_assertions(options.debug_assertions());
    state.db.set_panic_strategy(options.panic_strategy);
    state.db.set_crate_cfg(root, input.cfg.clone())?;
    state.db.override_crate_output(root)?;
    if let Some(directory) = &input.compilation_dir {
        state.db.set_crate_directory(root, directory.clone())?;
    }
    if let Some(environment) = &input.environment {
        state.db.set_crate_env(root, environment.clone())?;
    }
    let collected = timings.time("hir (includes module load/parse)", || {
        flux_hir::collect_source_on(
            &mut state.db,
            state.items,
            root,
            flux_intern::Symbol::intern(&input.name),
            imports,
        )
    });
    let own = collected.items.main_crate();
    let macro_freshness = libraries.macro_freshness();
    let configuration = configuration(
        input,
        options,
        kind,
        capability,
        &aliases,
        native,
        macro_freshness,
    )?;
    let own_indices = flux_hir::flib::CrateIndices::collect(&collected.items, own);
    let reused_library = |hash, minimum| {
        semantic::reusable_library(output, &checked_dependencies, libraries.cache).is_ok_and(
            |(manifest, recheck)| {
                let mut actual = manifest.dependencies;
                let mut expected: Vec<_> = state.dependencies.iter().map(|d| d.hash).collect();
                actual.sort_unstable();
                expected.sort_unstable();
                manifest.hash == hash
                    && manifest.name == input.name
                    && (metadata_only || manifest.linkable)
                    && actual == expected
                    && recheck >= minimum
            },
        )
    };
    let observation_count = collected.items.crates[own.0 as usize].observation_count();
    let pre_lower_hash = if recheck == Recheck::Collect && !collected.has_errors() {
        let hash = crate_hash_with_indices(
            &collected.items,
            own,
            &own_indices,
            &state.dependencies,
            &configuration,
        )?;
        if reused_library(hash, Recheck::Collect)
            && state.db.syntax_errors().is_empty()
            && crate::identifiers::diagnostics(&state.db).is_empty()
        {
            eprintln!("Reused {} after expansion", input.name);
            return Ok(true);
        }
        Some(hash)
    } else {
        None
    };
    let hir = timings.time("hir body lowering", || collected.lower(&state.db));
    let body_observed_inputs =
        hir.items.crates[own.0 as usize].observation_count() != observation_count;
    let hash = match pre_lower_hash {
        Some(hash) if !body_observed_inputs => hash,
        _ => crate_hash_with_indices(
            &hir.items,
            own,
            &own_indices,
            &state.dependencies,
            &configuration,
        )?,
    };
    let mut effective_native = options.native_link(native);
    for library in &hir.items.native_libraries {
        effective_native.add_library(*library);
    }
    // Lowering has already executed host macros and recorded their actual
    // tokens, hygiene and observations in `hash`. Do not bypass current errors.
    if state.db.syntax_errors().is_empty()
        && crate::identifiers::diagnostics(&state.db).is_empty()
        && hir.diagnostics.is_empty()
    {
        let reusable = if kind == CrateType::Bin {
            crate::artifact_cache::is_semantically_fresh(output, hash, &effective_native)
        } else {
            reused_library(hash, recheck.min(Recheck::LowerBodies))
        };
        if reusable {
            eprintln!("Reused {} after expansion", input.name);
            return Ok(true);
        }
    }
    let (loaded, imported) = if let Some(checked) = semantic_inputs {
        checked
    } else {
        // Use the same immutable archives and loading order. Their HIR gets the
        // same IDs on the fresh built-in prefix. Keep the source HIR and its
        // already executed macro calls rather than collecting the source again.
        let mut checked = timings.time("deferred dependency IR", || {
            DependencyState::new()?.load_inputs(
                &deferred.expect("IR is loaded or deferred"),
                false,
                defer_functions,
                link_dependency_objects,
            )
        })?;
        if checked.items.counts() != dependency_counts || checked.dependencies != state.dependencies
        {
            return Err("dependency mappings changed while loading deferred IR".into());
        }
        let semantic_inputs = prepare_semantics(&mut checked)?;
        state.objects = checked.objects;
        semantic_inputs
    };
    let hashes: Vec<_> = state
        .dependencies
        .iter()
        .map(|d| (d.krate, d.hash))
        .chain(std::iter::once((own, hash)))
        .collect();
    loaded.signatures().type_ids.set_crate_hashes(&hashes);
    let mut imported = if kind == CrateType::Lib
        && defer_functions
        && crate::builtins::active().is_some_and(|suite| suite.use_object(options.optimized()))
    {
        imported.share_builtin_templates()
    } else {
        imported
    };
    let roots = timings.time("imported call targets", || {
        imported.required_functions(&hir.items, loaded.signatures())
    });
    let typed = timings.time("types (includes const evaluation)", || {
        flux_mir::check_file_from(&hir, &loaded, &imported, &roots.functions)
    });
    if let Some(error) = imported.mir().deferred_error() {
        return Err(error.to_string());
    }
    let mut errors = state.db.syntax_errors();
    errors.extend(crate::identifiers::diagnostics(&state.db));
    errors.extend(hir.diagnostics.iter().cloned());
    errors.extend(typed.diagnostics.iter().cloned());
    crate::driver::report_warnings(&state.db, &typed.warnings);
    if kind == CrateType::Bin
        && let Some(error) = flux_ty::check_main(&hir.items, &typed.sigs)
    {
        errors.push(error);
    }
    if !errors.is_empty() {
        let mut reported = errors.clone();
        report_all(&state.db, &mut reported);
        let mut more = timings.time("mir checks on clean bodies", || {
            flux_mir::check_clean_bodies_from(&hir, &typed, &errors, &imported)
        });
        crate::driver::report_remaining(&state.db, &mut more, &reported);
        return Ok(false);
    }
    let local_mir = timings.time("mir build + checks", || {
        flux_mir::build_local_file_from_with_workers(
            &hir,
            &typed,
            &imported,
            &roots.functions,
            options.panic_strategy,
            crate::backend_workers::limit().get(),
        )
    });
    if let Some(error) = imported
        .mir()
        .deferred_error()
        .or_else(|| local_mir.deferred_error())
    {
        return Err(error.to_string());
    }
    errors.extend(local_mir.diagnostics.iter().cloned());
    if !errors.is_empty() {
        report_all(&state.db, &mut errors);
        return Ok(false);
    }
    let target = flux_codegen::Target::host().ok_or("unsupported host target")?;
    let imported_crates: Vec<_> = if whole_program {
        Vec::new()
    } else {
        loaded_hashes.iter().map(|&(krate, _)| krate).collect()
    };
    let builtin_object =
        crate::builtins::active().is_some_and(|suite| suite.use_object(options.optimized()));
    let specialization_inputs = specialization_cache
        .filter(|_| demand_cache.is_none())
        .filter(|_| !roots.instances.is_empty())
        .and_then(|cache| {
            let native = timings
                .time("native dependency symbols", || {
                    native_function_symbols(&state.objects)
                })
                .ok()?;
            let dependency_hashes = &hashes[..hashes.len() - 1];
            let object = cache.prepare(
                dependency_items.map(|items| (items, &loaded, &mut imported)),
                &native,
                None,
                flux_codegen::Compilation {
                    kind: flux_codegen::OutputKind::DependencySpecializations,
                    crate_hashes: dependency_hashes,
                    imported: &imported_crates,
                    builtin_object,
                    required_functions: &[],
                    required_instances: &[],
                },
                (target, options.object_options()),
                &mut timings,
            )?;
            Some((native, object))
        })
        .or_else(|| {
            let object = demand_cache.as_mut()?.take_object()?;
            Some((native_function_symbols(&state.objects).ok()?, object))
        });
    if let Some(error) = imported.mir().deferred_error() {
        return Err(error.to_string());
    }
    let mir = imported.finish(local_mir);
    let mut object_requirements = Vec::new();
    let function_summary = std::cell::OnceCell::new();
    let mut linked = None;
    let object = if metadata_only {
        None
    } else {
        // An intermediate library emits definitions reachable from its own
        // interface. The final executable loads the complete artifact graph
        // and supplies the outstanding requirements of every dependency.
        let (codegen_roots, required_instances) = if kind == CrateType::Bin {
            let functions: Vec<_> = roots
                .functions
                .iter()
                .copied()
                .filter(|function| !whole_program || function.into_raw() < hir.items.core.functions)
                .collect();
            (functions, roots.instances.as_slice())
        } else {
            (Vec::new(), &[][..])
        };
        // Whole-program lowering retains its ownership and cross-crate inlining.
        let compilation = (!whole_program || builtin_object || units.get() > 1).then_some(
            flux_codegen::Compilation {
                kind: match kind {
                    CrateType::Lib => flux_codegen::OutputKind::Library(own),
                    CrateType::Bin => flux_codegen::OutputKind::Executable,
                },
                crate_hashes: &hashes,
                imported: &imported_crates,
                required_functions: &codegen_roots,
                required_instances,
                builtin_object,
            },
        );
        if let Some((native_symbols, mut specializations)) = specialization_inputs {
            let result = (|| {
                let compilation =
                    compilation.expect("separate executable uses checked dependencies");
                let lower = |parts: Option<&mut flux_codegen::LoweringTimings>| {
                    flux_codegen::lower_with_native_definitions(
                        &hir,
                        &typed,
                        &mir,
                        flux_codegen::Compilation {
                            required_instances: &[],
                            ..compilation
                        },
                        flux_codegen::NativeDefinitions {
                            dependencies: &native_symbols,
                            specializations: Some(&specializations.symbols),
                            specialization_roots: None,
                        },
                        parts,
                    )
                };
                let mut module = if options.timings {
                    timings.time_lowering(|parts| lower(Some(parts)))
                } else {
                    lower(None)
                }
                .ok()?
                .module;
                let output_object =
                    compile_object(&mut module, target, options, &mut timings).ok()?;
                link_executable(
                    ExecutableLink {
                        output,
                        target,
                        name: &input.name,
                        options,
                        native: &effective_native,
                        dependencies: &state.objects,
                        objects: &[output_object],
                        specialization: Some(specializations.bytes()),
                    },
                    &mut timings,
                )
                .ok()
            })();
            if let Some(result) = result {
                specializations.publish(output, &mut timings);
                linked = Some(result);
            }
        }
        if linked.is_none()
            && let Some((cache, dependencies)) = demand_cache.zip(demand_inputs.as_ref())
            && let Some(compilation) = compilation
        {
            let demand = DemandSpecialization {
                cache,
                dependencies,
                compilation,
                codegen: (&hir, &typed, &mir),
            };
            linked = specialize_on_demand(
                demand,
                ExecutableLink {
                    output,
                    target,
                    name: &input.name,
                    options,
                    native: &effective_native,
                    dependencies: &state.objects,
                    objects: &[],
                    specialization: None,
                },
                &mut timings,
            )
            .ok();
        }
        if linked.is_some() {
            None
        } else {
            let lowered = if units.get() == 1
                && link_dependency_objects
                && !state.objects.is_empty()
                && native_cutoff_enabled()
            {
                let defined = timings.time("native dependency symbols", || {
                    native_function_symbols(&state.objects)
                })?;
                let compilation = compilation.expect("separate executable has native requirements");
                if options.timings {
                    timings.time_lowering(|parts| {
                        flux_codegen::lower_linked_executable(
                            &hir,
                            &typed,
                            &mir,
                            compilation,
                            &defined,
                            Some(parts),
                        )
                    })
                } else {
                    flux_codegen::lower_linked_executable(
                        &hir,
                        &typed,
                        &mir,
                        compilation,
                        &defined,
                        None,
                    )
                }
            } else if units.get() == 1 && options.timings {
                timings.time_lowering(|parts| {
                    flux_codegen::lower_with_optimization(
                        &hir,
                        &typed,
                        &mir,
                        compilation,
                        options.object_options().optimization,
                        Some(parts),
                    )
                })
            } else {
                timings.time("mono + keel lowering", || {
                    if let Some(compilation) = compilation
                        && units.get() > 1
                    {
                        return flux_codegen::lower_partitioned_crate(
                            &hir,
                            &typed,
                            &mir,
                            compilation,
                        );
                    }
                    flux_codegen::lower_with_optimization(
                        &hir,
                        &typed,
                        &mir,
                        compilation,
                        options.object_options().optimization,
                        None,
                    )
                })
            }
            .map_err(|mut errors| {
                report_all(&state.db, &mut errors);
                "cannot lower crate".to_owned()
            })?;
            let mut module = lowered.module;
            object_requirements = lowered.required_instances;
            if let Some(publish) = pipeline.as_ref().and_then(|stage| stage.publish) {
                let metadata = timings.time("pipeline metadata", || {
                    write_checked_with_indices(
                        CheckedCrate {
                            items: &hir.items,
                            sigs: &typed.sigs,
                            mir: &mir,
                            krate: own,
                            hash,
                            dependencies: &state.dependencies,
                            object: None,
                            required_instances: &object_requirements,
                            freshness: None,
                        },
                        &own_indices,
                        &[],
                        &function_summary,
                    )
                })?;
                publish(&metadata)?;
            }
            let object = if units.get() > 1 {
                timings.time("backend units (parallel inclusive)", || {
                    crate::backend_units::compile(
                        module,
                        units,
                        target,
                        output,
                        options.panic_strategy,
                    )
                })?
            } else {
                vec![compile_object(&mut module, target, options, &mut timings)?]
            };
            Some(object)
        }
    };
    if kind == CrateType::Bin {
        let (warnings, consumed_native, native_before) = match linked {
            Some(result) => result,
            None => link_executable(
                ExecutableLink {
                    output,
                    target,
                    name: &input.name,
                    options,
                    native: &effective_native,
                    dependencies: &state.objects,
                    objects: &object.expect("executable compilation emits an object"),
                    specialization: None,
                },
                &mut timings,
            )?,
        };
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
        let dependency_hashes: Vec<_> = state.dependencies.iter().map(|d| d.hash).collect();
        let freshness = Snapshot::capture_with_indices(
            &hir.items,
            own,
            &own_indices,
            &configuration,
            &dependency_hashes,
            macro_freshness,
        )?;
        // Resolve and hash again after linking. A changing archive or search
        // result must not authorize reuse of a link made from earlier inputs.
        let native_after = crate::native_link::Snapshot::capture(&effective_native).ok();
        let stable_native = match (native_before, native_after) {
            (Some(before), Some(after))
                if before == after && after.matches_consumed(&consumed_native) =>
            {
                Some(after)
            }
            _ => None,
        };
        crate::artifact_cache::record(output, freshness.as_ref(), hash, stable_native)?;
    } else {
        let dependency_hashes: Vec<_> = state.dependencies.iter().map(|d| d.hash).collect();
        let freshness = if input.host_macros.is_some() {
            Snapshot::capture_macro_wrapper(
                &hir.items,
                own,
                &own_indices,
                &configuration,
                &dependency_hashes,
            )
        } else {
            Snapshot::capture_with_indices(
                &hir.items,
                own,
                &own_indices,
                &configuration,
                &dependency_hashes,
                macro_freshness,
            )
        }?;
        // Only archives whose source executed macros need an expansion
        // recheck. Ordinary edits go directly to the full loader.
        let checked_dependencies = if !checked_dependencies.is_empty()
            && hir.items.crates[own.0 as usize].expanded_host_macros()
        {
            checked_dependencies.push(u8::from(!body_observed_inputs));
            checked_dependencies.as_slice()
        } else {
            &[]
        };
        let bytes = write_checked_with_indices(
            CheckedCrate {
                items: &hir.items,
                sigs: &typed.sigs,
                mir: &mir,
                krate: own,
                hash,
                dependencies: &state.dependencies,
                object: object.map(|mut objects| {
                    assert_eq!(objects.len(), 1, "library output has one object");
                    objects.pop().expect("one library object")
                }),
                required_instances: &object_requirements,
                freshness: freshness.as_ref(),
            },
            &own_indices,
            checked_dependencies,
            &function_summary,
        )?;
        let destination = pipeline.as_ref().map_or(output, |stage| stage.output);
        crate::flib::write_atomic(destination, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", destination.display()))?;
    }
    if options.timings {
        let (read, indexed) = mir.deferred_functions.chunk_counts();
        if indexed != 0 {
            eprintln!("dependency function chunks read {read}/{indexed}");
        }
    }
    // Large dependency graphs leave independent HIR, MIR and type tables to
    // release. Use a reserved slot once the output no longer borrows them.
    if workers > 1 && state.dependencies.len() >= 32 {
        std::thread::scope(|scope| {
            let _ = std::thread::Builder::new().spawn_scoped(scope, move || {
                drop((mir, typed, loaded));
            });
            drop((hir, state.db));
        });
    }
    Ok(true)
}

fn cache_specializations(
    options: BuildOptions,
    kind: CrateType,
    libraries: ExternalLibraries<'_>,
) -> bool {
    kind == CrateType::Bin
        && libraries.dependency_files.is_some()
        && libraries.cache.is_some()
        && crate::builtins::has_staged_libraries()
        && supports_dependency_specializations(options)
}

pub(crate) fn supports_dependency_specializations(options: BuildOptions) -> bool {
    cfg!(all(target_arch = "aarch64", target_os = "macos"))
        && options.optimize
        && !options.fast_build
        && !options.optimize_build_script
        && options.panic_strategy == flux_mir::PanicStrategy::Abort
        && native_cutoff_enabled()
}

fn large_specialization_graph(state: &DependencyState) -> bool {
    // Small graphs are cheaper to complete directly. Larger graphs can
    // provide many requirements from exports the link will discard.
    state.required_instances.len() >= 4096
}

/// Completes the concrete Rust requirements of the checked library graph.
pub(crate) fn prepare_library_specializations(
    library: &Path,
    files: &[&Path],
    artifacts: &semantic::FreshArtifacts,
    options: BuildOptions,
    executable: Option<&Path>,
) -> Option<specialization_object::Specializations> {
    crate::builtins::select_for_strategy(options.panic_strategy).ok()?;
    let _strategy = crate::builtins::strategy_scope(options.panic_strategy).ok()?;
    if !crate::builtins::has_staged_libraries() {
        return None;
    }
    crate::builtins::activate_selected().ok()?;
    let mut timings =
        crate::build::Timings::new(options.timings, "library specializations elapsed");
    let dependencies = timings.time("specialization dependency artifacts", || {
        let known = semantic::embedded_hashes().ok()?;
        semantic::DependencyInputs::resolve(
            &[library.to_owned()],
            &known,
            Some(files),
            Some(artifacts),
            semantic::ArchiveRead::Full,
        )
        .ok()
    })?;
    if !dependencies.all_linkable(None) {
        return None;
    }
    let checksums = dependencies.checksums();
    let cache = timings.time("library specialization cache read", || {
        specialization_object::Cache::open(library, &checksums, None)
    })?;
    let mut object = if cache.needs_build() {
        if let Some(output) = executable
            && specialization_object::Cache::open_demand(output, &checksums)
                .is_some_and(|cache| !cache.needs_build())
        {
            // The executable can try its graph-matched object directly.
            return None;
        }
        let mut state = timings
            .time("specialization dependency IR", || {
                semantic::DependencyState::new()?.load_inputs(
                    &dependencies,
                    false,
                    !cfg!(debug_assertions),
                    true,
                )
            })
            .ok()?;
        if executable.is_some() && large_specialization_graph(&state) {
            return None;
        }
        let native = native_function_symbols(&state.objects).ok()?;
        let (loaded, mut imported) = prepare_semantics(&mut state).ok()?;
        let hashes: Vec<_> = state
            .dependencies
            .iter()
            .map(|d| (d.krate, d.hash))
            .collect();
        let crates: Vec<_> = state
            .dependencies
            .iter()
            .filter_map(|d| {
                (state.items.module_of(d.krate).into_raw() >= state.items.core.modules)
                    .then_some(d.krate)
            })
            .collect();
        if options.timings {
            eprintln!("Preparing native specializations for {}", library.display());
        }
        cache.prepare(
            Some((state.items, &loaded, &mut imported)),
            &native,
            None,
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::DependencySpecializations,
                crate_hashes: &hashes,
                imported: &crates,
                builtin_object: crate::builtins::active()
                    .is_some_and(|suite| suite.use_object(true)),
                required_functions: &[],
                required_instances: &[],
            },
            (flux_codegen::Target::host()?, options.object_options()),
            &mut timings,
        )?
    } else {
        // The cached object already carries the exact validated graph key.
        cache.into_object()?
    };
    object.publish(library, &mut timings);
    Some(object)
}

fn compile_object(
    module: &mut flux_codegen::Module,
    target: flux_codegen::Target,
    options: BuildOptions,
    timings: &mut crate::build::Timings,
) -> Result<Vec<u8>, String> {
    let result = if options.timings {
        let (result, backend) = timings.time("backend (inclusive)", || {
            flux_codegen::compile_object_with_options_and_timings(
                module,
                target,
                options.object_options(),
            )
        });
        for (name, duration) in [
            ("  backend verify", backend.verify),
            ("  backend optimize", backend.optimize),
            ("  backend native codegen", backend.codegen),
            ("  backend assemble", backend.assemble),
        ] {
            if let Some(duration) = duration {
                timings.record(name, duration);
            }
        }
        result
    } else {
        flux_codegen::compile_object_with_options(module, target, options.object_options())
    };
    result.map_err(|e| e.to_string())
}

struct ExecutableLink<'a> {
    output: &'a Path,
    target: flux_codegen::Target,
    name: &'a str,
    options: BuildOptions,
    native: &'a flux_codegen::NativeLink,
    dependencies: &'a [(std::path::PathBuf, Vec<u8>)],
    objects: &'a [Vec<u8>],
    specialization: Option<&'a [u8]>,
}

type LinkedExecutable = (
    Vec<String>,
    flux_codegen::NativeLinkInputs,
    Option<crate::native_link::Snapshot>,
);

struct DemandSpecialization<'a> {
    cache: specialization_object::Cache,
    dependencies: &'a semantic::DependencyInputs,
    compilation: flux_codegen::Compilation<'a>,
    codegen: (
        &'a flux_hir::Hir,
        &'a flux_ty::TypedFile,
        &'a flux_mir::FileMir,
    ),
}

fn specialize_on_demand(
    demand: DemandSpecialization<'_>,
    link: ExecutableLink<'_>,
    timings: &mut crate::build::Timings,
) -> Result<LinkedExecutable, String> {
    let native = native_function_symbols(link.dependencies)?;
    let calls = undefined_symbols(link.dependencies.iter().map(|(_, bytes)| bytes.as_slice()))?;
    let calls = calls
        .iter()
        .filter_map(|name| name.strip_prefix('_'))
        .map(str::to_owned)
        .collect();
    let (hir, typed, mir) = demand.codegen;
    let lower = |symbols: &HashSet<String>| {
        flux_codegen::lower_with_native_definitions(
            hir,
            typed,
            mir,
            flux_codegen::Compilation {
                required_instances: &[],
                ..demand.compilation
            },
            flux_codegen::NativeDefinitions {
                dependencies: &native,
                specializations: Some(symbols),
                specialization_roots: None,
            },
            None,
        )
        .map(|lowered| lowered.module)
        .map_err(|_| String::from("cannot lower executable"))
    };
    // The first executable object exposes dependency calls and their native
    // reachability. Never publish or execute a probe image.
    let mut scout = timings.time("specialization probe lowering", || lower(&calls))?;
    let scout = [compile_object(
        &mut scout,
        link.target,
        link.options,
        timings,
    )?];
    let probe_link = ExecutableLink {
        objects: &scout,
        ..link
    };
    let names = timings.time("specialization probe link", || {
        with_executable_inputs(&probe_link, |inputs| {
            probe_specialization_imports(inputs, probe_link.native)
        })
    })?;
    if names.is_empty() {
        return link_executable(probe_link, timings);
    }
    let mut state = timings.time("demand specialization dependency IR", || {
        DependencyState::new()?.load_inputs(
            demand.dependencies,
            false,
            !cfg!(debug_assertions),
            false,
        )
    })?;
    let (loaded, mut imported) = prepare_semantics(&mut state)?;
    let hashes = demand.compilation.crate_hashes;
    let mut specialized = demand
        .cache
        .prepare(
            Some((state.items, &loaded, &mut imported)),
            &native,
            Some(&names),
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::DependencySpecializations,
                crate_hashes: &hashes[..hashes.len() - 1],
                required_functions: &[],
                ..demand.compilation
            },
            (link.target, link.options.object_options()),
            timings,
        )
        .ok_or("cannot lower required dependency specializations")?;
    let mut module = timings.time("mono + keel lowering", || lower(&specialized.symbols))?;
    let objects = [compile_object(
        &mut module,
        link.target,
        link.options,
        timings,
    )?];
    let result = link_executable(
        ExecutableLink {
            objects: &objects,
            specialization: Some(specialized.bytes()),
            ..link
        },
        timings,
    )?;
    specialized.publish(&specialization_object::demand_path(link.output), timings);
    Ok(result)
}

fn undefined_symbols<'a>(
    objects: impl Iterator<Item = &'a [u8]>,
) -> Result<HashSet<String>, String> {
    use object::{Object, ObjectSymbol};

    let mut undefined = HashSet::new();
    for bytes in objects {
        let object = object::File::parse(bytes).map_err(|error| error.to_string())?;
        undefined.extend(
            object
                .symbols()
                .filter(|symbol| symbol.is_undefined())
                .filter_map(|symbol| symbol.name().ok())
                .map(str::to_owned),
        );
    }
    Ok(undefined)
}

fn probe_specialization_imports(
    inputs: &[flux_codegen::ObjectInput<'_>],
    native: &flux_codegen::NativeLink,
) -> Result<HashSet<String>, String> {
    use object::Object;
    const CANDIDATE_LIBRARY: &str = "/flux-specialization-probe.dylib";

    let names = undefined_symbols(inputs.iter().map(|input| input.bytes))?;
    let bytes =
        flux_codegen::probe_specialization_imports(inputs, native, CANDIDATE_LIBRARY, &names)
            .map_err(|error| error.to_string())?;
    let image = object::File::parse(bytes.as_slice()).map_err(|error| error.to_string())?;
    let imports = image.imports().map_err(|error| error.to_string())?;
    Ok(imports
        .iter()
        .filter(|import| import.library() == CANDIDATE_LIBRARY.as_bytes())
        .filter_map(|import| std::str::from_utf8(import.name()).ok()?.strip_prefix('_'))
        .map(str::to_owned)
        .collect())
}

fn with_executable_inputs<T>(
    link: &ExecutableLink<'_>,
    f: impl FnOnce(&[flux_codegen::ObjectInput<'_>]) -> T,
) -> T {
    let names: Vec<_> = (0..link.objects.len())
        .map(|index| format!("{}-{index}", link.name))
        .collect();
    let mut inputs: Vec<_> = link
        .objects
        .iter()
        .zip(&names)
        .map(|(bytes, name)| flux_codegen::ObjectInput { name, bytes })
        .collect();
    if let Some(suite) =
        crate::builtins::active().filter(|s| s.use_object(link.options.optimized()))
    {
        inputs.push(flux_codegen::ObjectInput {
            name: "flux-builtins",
            bytes: suite
                .object
                .as_deref()
                .expect("selected suite object exists"),
        });
    }
    let names: Vec<_> = link
        .dependencies
        .iter()
        .map(|(path, _)| path.to_string_lossy())
        .collect();
    inputs.extend(
        link.dependencies
            .iter()
            .zip(&names)
            .map(|((_, bytes), name)| flux_codegen::ObjectInput { name, bytes }),
    );
    if let Some(bytes) = link.specialization {
        inputs.push(flux_codegen::ObjectInput {
            name: "flux-dependency-specializations",
            bytes,
        });
    }
    f(&inputs)
}

fn link_executable(
    link: ExecutableLink<'_>,
    timings: &mut crate::build::Timings,
) -> Result<LinkedExecutable, String> {
    let native_before = crate::native_link::Snapshot::capture(link.native).ok();
    let (warnings, consumed_native) = timings
        .time("link", || {
            with_executable_inputs(&link, |inputs| {
                flux_codegen::link_objects_with_inputs(
                    inputs,
                    link.target,
                    link.output,
                    link.native,
                )
            })
        })
        .map_err(|e| e.to_string())?;
    Ok((warnings, consumed_native, native_before))
}

fn prepare_semantics(
    state: &mut DependencyState,
) -> Result<(flux_ty::flib::LoadedTypes, flux_mir::imported::ImportedMir), String> {
    let loaded_metadata: Vec<_> = state
        .dependencies
        .iter()
        .filter(|d| {
            state.items.has_artifact_builtins()
                || state.items.crates[d.krate.0 as usize].root.into_raw()
                    >= state.items.core.modules
        })
        .map(|d| flux_ty::flib::LoadedCrate {
            krate: d.krate,
            hash: d.hash,
            indices: &d.indices,
        })
        .collect();
    let loaded = flux_ty::flib::LoadedTypes::from_metadata(
        &state.items,
        std::mem::take(&mut state.sigs),
        &loaded_metadata,
    )
    .map_err(|e| e.to_string())?;
    let imported = if let Some(suite) = crate::builtins::active() {
        flux_mir::imported::ImportedMir::with_builtin_prefix(
            &state.items,
            std::mem::take(&mut state.mir),
            suite.mir.clone(),
        )
    } else {
        flux_mir::imported::ImportedMir::new(std::mem::take(&mut state.mir), state.items.counts())
    }
    .and_then(|provider| {
        provider.with_required_instances(std::mem::take(&mut state.required_instances))
    })
    .map(|provider| provider.with_function_summaries(std::mem::take(&mut state.function_summaries)))
    .map_err(|e| e.to_string())?;
    Ok((loaded, imported))
}

// The trial and its separate-link control share the same library objects.
fn experimental_native_mode() -> Option<std::ffi::OsString> {
    std::env::var_os("FLUX_EXPERIMENTAL_NATIVE_LINK")
}

fn native_cutoff_enabled() -> bool {
    experimental_native_mode().is_none_or(|mode| mode != "separate")
}

pub(crate) fn whole_program_native(native: &flux_codegen::NativeLink) -> bool {
    *native != Default::default()
        && !experimental_native_mode().is_some_and(|mode| mode == "separate" || mode == "cutoff")
}

fn native_function_symbols(
    objects: &[(std::path::PathBuf, Vec<u8>)],
) -> Result<std::collections::HashSet<String>, String> {
    use object::{Object, ObjectSymbol};

    let mut defined = std::collections::HashSet::new();
    for (path, bytes) in objects {
        let object = object::File::parse(bytes.as_slice()).map_err(|error| {
            format!(
                "cannot read native symbols from {}: {error}",
                path.display()
            )
        })?;
        for symbol in object.symbols().filter(|symbol| {
            symbol.is_definition()
                && !symbol.is_local()
                && symbol.kind() == object::SymbolKind::Text
        }) {
            let Ok(name) = symbol.name() else { continue };
            let name = if object.format() == object::BinaryFormat::MachO {
                name.strip_prefix('_').unwrap_or(name)
            } else {
                name
            };
            defined.insert(name.to_owned());
        }
    }
    Ok(defined)
}

fn configuration(
    input: &SourceInput,
    options: BuildOptions,
    kind: CrateType,
    capability: Capability,
    aliases: &[(String, [u8; 32])],
    native: &flux_codegen::NativeLink,
    macro_freshness: MacroFreshness,
) -> Result<Vec<u8>, String> {
    let whole_program = kind == CrateType::Bin && whole_program_native(native);
    let effective_native = options.native_link(native);
    let native = &effective_native;
    let mut e = Encoder::new();
    let encode = |result: Result<(), flux_hir::flib::Error>| result.map_err(|e| e.to_string());
    encode("FLIB-REQUEST-v2".encode(&mut e))?;
    encode((macro_freshness == MacroFreshness::Cargo).encode(&mut e))?;
    encode(input.file.encode(&mut e))?;
    encode(input.name.encode(&mut e))?;
    encode(input.package_identity.encode(&mut e))?;
    encode(input.edition.encode(&mut e))?;
    encode(input.compilation_dir.encode(&mut e))?;
    encode(
        std::env::current_dir()
            .map_err(|e| e.to_string())?
            .encode(&mut e),
    )?;
    encode(input.cfg.names.encode(&mut e))?;
    encode(input.cfg.values.encode(&mut e))?;
    encode(input.environment.encode(&mut e))?;
    encode(input.host_macros.is_some().encode(&mut e))?;
    if let Some(exports) = &input.host_macros {
        encode((exports.len() as u32).encode(&mut e))?;
        for export in exports {
            encode(export.name.encode(&mut e))?;
            encode(export.kind.encode(&mut e))?;
            encode(export.helpers.encode(&mut e))?;
            let descriptor = export
                .expander
                .flib_descriptor()
                .ok_or("procedural macro host has no artifact descriptor")?;
            encode(descriptor.encode(&mut e))?;
        }
    }

    let builtin_object = crate::builtins::object_hash(options.optimized()).map(<[u8]>::to_vec);
    encode(builtin_object.encode(&mut e))?;
    encode(
        crate::native_link::Request::from(native)
            .configuration()?
            .encode(&mut e),
    )?;
    encode(whole_program.encode(&mut e))?;
    encode(options.optimize.encode(&mut e))?;
    encode(options.fast_build.encode(&mut e))?;
    encode(options.optimize_build_script.encode(&mut e))?;
    encode(
        crate::backend_units::count(options.fast_build && kind == CrateType::Bin)?
            .get()
            .encode(&mut e),
    )?;
    encode(options.debug_assertions().encode(&mut e))?;
    encode(options.panic_strategy.encode(&mut e))?;
    encode(
        match kind {
            CrateType::Bin => 0u8,
            CrateType::Lib => 1u8,
        }
        .encode(&mut e),
    )?;
    encode(
        match capability {
            Capability::Metadata => 0u8,
            Capability::Object => 1u8,
        }
        .encode(&mut e),
    )?;
    let target = flux_codegen::Target::host().ok_or("unsupported flux artifact target")?;
    encode(target.triple().encode(&mut e))?;
    let mut aliases: Vec<_> = aliases
        .iter()
        .map(|(name, hash)| (name, hash.to_vec()))
        .collect();
    aliases.sort();
    encode(aliases.encode(&mut e))?;
    if kind == CrateType::Bin
        && !whole_program
        && !aliases.is_empty()
        && crate::backend_units::count(options.fast_build)?.get() == 1
        && native_cutoff_enabled()
    {
        encode("native-cutoff-v1".encode(&mut e))?;
    }
    Ok(e.into_bytes())
}

/// Checks the exact source/configuration/dependency snapshot before reusing a package artifact.
pub(crate) fn is_fresh(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    externs: &[(String, std::path::PathBuf)],
) -> Result<bool, String> {
    is_fresh_with_capability(
        input,
        output,
        options,
        ExternalLibraries {
            externs,
            dependency_files: None,
            cache: None,
        },
        false,
        &Default::default(),
    )
}

pub(crate) fn is_fresh_with_capability(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    libraries: ExternalLibraries<'_>,
    metadata_only: bool,
    artifacts: &crate::flib::semantic::FreshArtifacts,
) -> Result<bool, String> {
    let Ok(artifact) = artifacts.read(output) else {
        return Ok(false);
    };
    let (config, dependencies) = freshness_configuration(
        input,
        options,
        libraries,
        CrateType::Lib,
        &Default::default(),
        metadata_only,
        artifacts,
    )?;
    artifact.is_fresh(
        &config,
        &dependencies,
        input.environment.as_ref(),
        if metadata_only {
            Capability::Metadata
        } else {
            Capability::Object
        },
    )
}

/// Checks an executable against its exact compiler, configuration, inputs and dependencies.
pub(crate) fn is_binary_fresh(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    libraries: ExternalLibraries<'_>,
    native: &flux_codegen::NativeLink,
    artifacts: &crate::flib::semantic::FreshArtifacts,
) -> Result<bool, String> {
    let (config, dependencies) = freshness_configuration(
        input,
        options,
        libraries,
        CrateType::Bin,
        native,
        false,
        artifacts,
    )?;
    crate::artifact_cache::is_fresh(output, &config, &dependencies, input.environment.as_ref())
}

fn freshness_configuration(
    input: &SourceInput,
    options: BuildOptions,
    libraries: ExternalLibraries<'_>,
    kind: CrateType,
    native: &flux_codegen::NativeLink,
    metadata_only: bool,
    artifacts: &crate::flib::semantic::FreshArtifacts,
) -> Result<(Vec<u8>, Vec<[u8; 32]>), String> {
    options.validate()?;
    let _strategy = crate::builtins::strategy_scope(options.panic_strategy)?;
    let mut dependencies = crate::flib::semantic::embedded_hashes()?;
    let externs = libraries.externs;
    let paths: Vec<_> = externs.iter().map(|(_, path)| path.clone()).collect();
    let identities = artifacts.dependencies(&paths, &dependencies, libraries.dependency_files)?;
    dependencies.extend(identities.ordered);
    let aliases: Vec<_> = externs
        .iter()
        .zip(identities.explicit)
        .map(|((alias, _), hash)| (alias.clone(), hash))
        .collect();
    let config = configuration(
        input,
        options,
        kind,
        if metadata_only {
            Capability::Metadata
        } else {
            Capability::Object
        },
        &aliases,
        native,
        libraries.macro_freshness(),
    )?;
    Ok((config, dependencies))
}

#[cfg(test)]
mod tests {
    #[test]
    fn automatic_unwind_linking_does_not_hide_whole_program_configuration() {
        let input = super::SourceInput::standalone(std::path::Path::new("input.rs"), "input");
        let options = super::BuildOptions {
            optimize: false,
            fast_build: false,
            optimize_build_script: false,
            timings: false,
            debug_assertions: None,
            panic_strategy: flux_syntax::PanicStrategy::Unwind,
        };
        let native = flux_codegen::NativeLink::default();
        let explicit = options.native_link(&native);
        assert_eq!(options.native_link(&native), options.native_link(&explicit));
        let key = |native| {
            super::configuration(
                &input,
                options,
                super::CrateType::Bin,
                super::Capability::Object,
                &[],
                native,
                super::MacroFreshness::Reexpand,
            )
            .unwrap()
        };
        assert_ne!(key(&native), key(&explicit));
    }
}
