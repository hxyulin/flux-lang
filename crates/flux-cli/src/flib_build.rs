//! Driver for explicit Fluxlib compilation.

use crate::build::SourceInput;
use crate::flib::semantic::Capability;
use crate::flib::semantic::{
    self, CheckedCrate, DependencyState, crate_hash_with_indices, write_checked_with_indices,
};
use crate::{BuildOptions, CrateType, report_all};
use flux_hir::flib::{Encode, Encoder};
use std::path::Path;

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

pub(crate) fn compile_native(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    kind: CrateType,
    metadata_only: bool,
    libraries: ExternalLibraries<'_>,
    native: &flux_codegen::NativeLink,
) -> Result<bool, String> {
    options.validate()?;
    let _strategy = crate::builtins::compilation_scope(options.panic_strategy)?;
    let units = crate::backend_units::count(options.fast_build && kind == CrateType::Bin)?;
    let whole_program = kind == CrateType::Bin && *native != Default::default();
    let mut timings = crate::build::Timings::new(options.timings, "compile scoped elapsed");
    let capability = if metadata_only {
        Capability::Metadata
    } else {
        Capability::Object
    };
    let externs = libraries.externs;
    let paths: Vec<_> = externs.iter().map(|(_, path)| path.clone()).collect();
    use semantic::Recheck;
    let (mut state, deferred, mut checked_dependencies, all_linkable, recheck) =
        timings.time("dependency artifact load", || {
            let state = crate::flib::time("dependency state initialization", DependencyState::new)?;
            let dependencies =
                state.resolve(&paths, libraries.dependency_files, libraries.cache)?;
            let all_linkable = dependencies.all_linkable();
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
            let defer_ir = recheck != Recheck::Full;
            let state = state.load_inputs(&dependencies, defer_ir)?;
            Ok::<_, String>((
                state,
                defer_ir.then_some(dependencies),
                checked_dependencies,
                all_linkable,
                recheck,
            ))
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
    let configuration = configuration(input, options, kind, capability, &aliases, native)?;
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
            DependencyState::new()?.load_inputs(&deferred.expect("IR is loaded or deferred"), false)
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
    let roots = timings.time("imported call targets", || {
        imported.required_functions(&hir.items, loaded.signatures())
    });
    let typed = timings.time("types (includes const evaluation)", || {
        flux_mir::check_file_from(&hir, &loaded, &imported, &roots.functions)
    });
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
    let mir = timings.time("mir build + checks", || {
        flux_mir::build_file_from_owned_with_panic_strategy(
            &hir,
            &typed,
            imported,
            &roots.functions,
            options.panic_strategy,
        )
    });
    errors.extend(mir.diagnostics.iter().cloned());
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
    let mut object_requirements = Vec::new();
    let object = if metadata_only {
        None
    } else {
        let builtin_object =
            crate::builtins::active().is_some_and(|suite| suite.use_object(options.optimized()));
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
        let lowered = timings
            .time("mono + keel lowering", || {
                if whole_program && !builtin_object && units.get() == 1 {
                    // Preserve whole-program ownership and cross-crate inlining.
                    return flux_codegen::lower_module(&hir, &typed, &mir).map(|module| {
                        flux_codegen::LoweredCrate {
                            module,
                            required_instances: Vec::new(),
                        }
                    });
                }
                let lower = if units.get() > 1 {
                    flux_codegen::lower_partitioned_crate
                } else {
                    flux_codegen::lower_crate_with_requirements
                };
                lower(
                    &hir,
                    &typed,
                    &mir,
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
                )
            })
            .map_err(|mut errors| {
                report_all(&state.db, &mut errors);
                "cannot lower crate".to_owned()
            })?;
        let mut module = lowered.module;
        object_requirements = lowered.required_instances;
        let object = if units.get() > 1 {
            timings.time("backend units (parallel inclusive)", || {
                crate::backend_units::compile(module, units, target, output, options.panic_strategy)
            })?
        } else {
            let object = if options.timings {
                let (result, backend) = timings.time("backend (inclusive)", || {
                    flux_codegen::compile_object_with_options_and_timings(
                        &mut module,
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
                flux_codegen::compile_object_with_options(
                    &mut module,
                    target,
                    options.object_options(),
                )
            };
            vec![object.map_err(|e| e.to_string())?]
        };
        Some(object)
    };
    if kind == CrateType::Bin {
        let object = object.expect("executable compilation emits an object");
        let names: Vec<_> = (0..object.len())
            .map(|index| format!("{}-{index}", input.name))
            .collect();
        let mut inputs: Vec<_> = object
            .iter()
            .zip(&names)
            .map(|(bytes, name)| flux_codegen::ObjectInput { name, bytes })
            .collect();
        if let Some(suite) = crate::builtins::active().filter(|s| s.use_object(options.optimized()))
        {
            inputs.push(flux_codegen::ObjectInput {
                name: "flux-builtins",
                bytes: suite
                    .object
                    .as_deref()
                    .expect("selected suite object exists"),
            });
        }
        let names: Vec<_> = state
            .objects
            .iter()
            .map(|(path, _)| path.to_string_lossy().into_owned())
            .collect();
        inputs.extend(
            state
                .objects
                .iter()
                .filter(|_| !whole_program)
                .zip(&names)
                .map(|((_, bytes), name)| flux_codegen::ObjectInput { name, bytes }),
        );
        let native_before = crate::native_link::Snapshot::capture(&effective_native).ok();
        let (warnings, consumed_native) = timings
            .time("link", || {
                flux_codegen::link_objects_with_inputs(&inputs, target, output, &effective_native)
            })
            .map_err(|e| e.to_string())?;
        for warning in warnings {
            eprintln!("warning: {warning}");
        }
        let dependency_hashes: Vec<_> = state.dependencies.iter().map(|d| d.hash).collect();
        let freshness = crate::flib::freshness::Snapshot::capture_with_indices(
            &hir.items,
            own,
            &own_indices,
            &configuration,
            &dependency_hashes,
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
        use crate::flib::freshness::Snapshot;
        let capture = if input.host_macros.is_some() {
            Snapshot::capture_macro_wrapper
        } else {
            Snapshot::capture_with_indices
        };
        let freshness = capture(
            &hir.items,
            own,
            &own_indices,
            &configuration,
            &dependency_hashes,
        )?;
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
        )?;
        crate::flib::write_atomic(output, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    }
    Ok(true)
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
    .map_err(|e| e.to_string())?;
    Ok((loaded, imported))
}

fn configuration(
    input: &SourceInput,
    options: BuildOptions,
    kind: CrateType,
    capability: Capability,
    aliases: &[(String, [u8; 32])],
    native: &flux_codegen::NativeLink,
) -> Result<Vec<u8>, String> {
    let whole_program = kind == CrateType::Bin && *native != Default::default();
    let effective_native = options.native_link(native);
    let native = &effective_native;
    let mut e = Encoder::new();
    let encode = |result: Result<(), flux_hir::flib::Error>| result.map_err(|e| e.to_string());
    encode("FLIB-REQUEST-v1".encode(&mut e))?;
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

    let builtin_object = crate::builtins::object_link_hash(options.optimized()).map(<[u8]>::to_vec);
    encode(builtin_object.encode(&mut e))?;
    encode(
        crate::native_link::Request::from(native)
            .configuration()?
            .encode(&mut e),
    )?;
    encode(whole_program.encode(&mut e))?;
    encode(options.optimize.encode(&mut e))?;
    encode(options.fast_build.encode(&mut e))?;
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
            )
            .unwrap()
        };
        assert_ne!(key(&native), key(&explicit));
    }
}
