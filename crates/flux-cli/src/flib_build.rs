//! Driver for explicit Fluxlib compilation.

use crate::build::SourceInput;
use crate::flib::semantic::Capability;
use crate::flib::semantic::{CheckedCrate, crate_hash_with_indices, write_checked_with_indices};
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
    let mut timings = crate::build::Timings::new(options.timings, "compile scoped elapsed");
    let capability = if metadata_only {
        Capability::Metadata
    } else {
        Capability::Object
    };
    let paths: Vec<_> = externs.iter().map(|(_, path)| path.clone()).collect();
    let mut state = timings.time("dependency artifact load", || {
        crate::flib::time(
            "dependency state initialization",
            crate::flib::semantic::DependencyState::new,
        )?
        .load(&paths)
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
    if capability == Capability::Object && state.objects.len() != loaded_hashes.len() {
        return Err("a dependency Fluxlib has no native object".into());
    }
    let loaded_metadata: Vec<_> = state
        .dependencies
        .iter()
        .filter(|d| {
            state.items.has_artifact_builtins()
                || loaded_hashes.iter().any(|&(id, _)| d.krate == id)
        })
        .map(|d| flux_ty::flib::LoadedCrate {
            krate: d.krate,
            hash: d.hash,
            indices: &d.indices,
        })
        .collect();
    let loaded =
        flux_ty::flib::LoadedTypes::from_metadata(&state.items, state.sigs, &loaded_metadata)
            .map_err(|e| e.to_string())?;
    let imported = if let Some(suite) = crate::builtins::active() {
        flux_mir::imported::ImportedMir::with_builtin_prefix(
            &state.items,
            state.mir,
            suite.mir.clone(),
        )
    } else {
        flux_mir::imported::ImportedMir::new(state.mir, state.items.counts())
    }
    .and_then(|provider| provider.with_required_instances(state.required_instances))
    .map_err(|e| e.to_string())?;
    let root = if let Some(exports) = &input.host_macros {
        state
            .db
            .add_proc_macro_crate(input.file.clone(), exports.clone())
    } else {
        state.db.load_with_edition(&input.file, input.edition)?
    };
    state.db.set_debug_assertions(options.debug_assertions());
    state.db.set_crate_cfg(root, input.cfg.clone())?;
    if let Some(directory) = &input.compilation_dir {
        state.db.set_crate_directory(root, directory.clone())?;
    }
    if let Some(environment) = &input.environment {
        state.db.set_crate_env(root, environment.clone())?;
    }
    let hir = timings.time("hir (includes module load/parse)", || {
        flux_hir::lower_source_on(
            &mut state.db,
            state.items,
            root,
            flux_intern::Symbol::intern(&input.name),
            imports,
        )
    });
    let own = hir.items.main_crate();
    let configuration = configuration(input, options, kind, capability, &aliases)?;
    let own_indices = flux_hir::flib::CrateIndices::collect(&hir.items, own);
    let hash = crate_hash_with_indices(
        &hir.items,
        own,
        &own_indices,
        &state.dependencies,
        &configuration,
    )?;
    let hashes: Vec<_> = state
        .dependencies
        .iter()
        .map(|d| (d.krate, d.hash))
        .chain(std::iter::once((own, hash)))
        .collect();
    loaded.signatures().type_ids.set_crate_hashes(&hashes);
    let roots = imported.required_functions(&hir.items, loaded.signatures());
    let typed = timings.time("types (includes const evaluation)", || {
        flux_mir::check_file_from(&hir, &loaded, &imported, &roots.functions)
    });
    let mut errors = state.db.syntax_errors();
    errors.extend(crate::identifiers::diagnostics(&state.db));
    errors.extend(hir.diagnostics.iter().cloned());
    errors.extend(typed.diagnostics.iter().cloned());
    if kind == CrateType::Bin
        && let Some(error) = flux_ty::check_main(&hir.items, &typed.sigs)
    {
        errors.push(error);
    }
    if !errors.is_empty() {
        errors.extend(flux_mir::check_clean_bodies_from(
            &hir, &typed, &errors, &imported,
        ));
        report_all(&state.db, &mut errors);
        return Ok(false);
    }
    let mir = timings.time("mir build + checks", || {
        flux_mir::build_file_from_owned(&hir, &typed, imported, &roots.functions)
    });
    errors.extend(mir.diagnostics.iter().cloned());
    if !errors.is_empty() {
        report_all(&state.db, &mut errors);
        return Ok(false);
    }
    let target = flux_codegen::Target::host().ok_or("unsupported host target")?;
    let imported_crates: Vec<_> = loaded_hashes.iter().map(|&(krate, _)| krate).collect();
    let mut object_requirements = Vec::new();
    let object = if metadata_only {
        None
    } else {
        let lowered = timings
            .time("mono + keel lowering", || {
                flux_codegen::lower_crate_with_requirements(
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
                        required_functions: &roots.functions,
                        required_instances: &roots.instances,
                        builtin_object: crate::builtins::active()
                            .is_some_and(|s| s.use_object(options.optimize)),
                    },
                )
            })
            .map_err(|mut errors| {
                report_all(&state.db, &mut errors);
                "cannot lower crate".to_owned()
            })?;
        let mut module = lowered.module;
        object_requirements = lowered.required_instances;
        let tier = if options.optimize {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
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
                    timings.record(name, duration);
                }
            }
            result
        } else {
            flux_codegen::compile_object(&mut module, target, tier)
        };
        Some(object.map_err(|e| e.to_string())?)
    };
    if kind == CrateType::Bin {
        let object = object.expect("executable compilation emits an object");
        let mut inputs = vec![flux_codegen::ObjectInput {
            name: &input.name,
            bytes: &object,
        }];
        if let Some(suite) = crate::builtins::active().filter(|s| s.use_object(options.optimize)) {
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
                .zip(&names)
                .map(|((_, bytes), name)| flux_codegen::ObjectInput { name, bytes }),
        );
        for warning in timings
            .time("link", || {
                flux_codegen::link_objects(
                    &inputs,
                    target,
                    output,
                    &flux_codegen::NativeLink::libraries(&hir.items.native_libraries),
                )
            })
            .map_err(|e| e.to_string())?
        {
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
        // Native library contents are not part of the HIR input trace.
        crate::artifact_cache::record(
            output,
            freshness
                .as_ref()
                .filter(|_| hir.items.native_libraries.is_empty()),
        )?;
    } else {
        let dependency_hashes: Vec<_> = state.dependencies.iter().map(|d| d.hash).collect();
        let freshness = crate::flib::freshness::Snapshot::capture_with_indices(
            &hir.items,
            own,
            &own_indices,
            &configuration,
            &dependency_hashes,
        )?;
        let bytes = write_checked_with_indices(
            CheckedCrate {
                items: &hir.items,
                sigs: &typed.sigs,
                mir: &mir,
                krate: own,
                hash,
                dependencies: &state.dependencies,
                object,
                required_instances: &object_requirements,
                freshness: freshness.as_ref(),
            },
            &own_indices,
        )?;
        crate::flib::write_atomic(output, &bytes)
            .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
    }
    Ok(true)
}

fn configuration(
    input: &SourceInput,
    options: BuildOptions,
    kind: CrateType,
    capability: Capability,
    aliases: &[(String, [u8; 32])],
) -> Result<Vec<u8>, String> {
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

    encode(options.optimize.encode(&mut e))?;
    encode(options.debug_assertions().encode(&mut e))?;
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
    let Ok(bytes) = std::fs::read(output) else {
        return Ok(false);
    };
    let (config, dependencies) = freshness_configuration(input, options, externs, CrateType::Lib)?;
    crate::flib::semantic::is_fresh(
        &bytes,
        &config,
        &dependencies,
        input.environment.as_ref(),
        Capability::Object,
    )
}

/// Checks an executable against its exact compiler, configuration, inputs and dependencies.
pub(crate) fn is_binary_fresh(
    input: &SourceInput,
    output: &Path,
    options: BuildOptions,
    externs: &[(String, std::path::PathBuf)],
) -> Result<bool, String> {
    let (config, dependencies) = freshness_configuration(input, options, externs, CrateType::Bin)?;
    crate::artifact_cache::is_fresh(output, &config, &dependencies, input.environment.as_ref())
}

fn freshness_configuration(
    input: &SourceInput,
    options: BuildOptions,
    externs: &[(String, std::path::PathBuf)],
    kind: CrateType,
) -> Result<(Vec<u8>, Vec<[u8; 32]>), String> {
    let known = crate::flib::semantic::DependencyState::new()?.dependencies;
    let paths: Vec<_> = externs.iter().map(|(_, path)| path.clone()).collect();
    let artifacts = crate::flib::semantic::resolve_paths(&paths, &known)?;
    let mut dependencies: Vec<_> = known.iter().map(|d| d.hash).collect();
    dependencies.extend(artifacts.iter().map(|a| a.manifest.hash));
    let mut aliases = Vec::new();
    for (alias, path) in externs {
        let data = std::fs::read(path).map_err(|e| e.to_string())?;
        aliases.push((alias.clone(), crate::flib::semantic::manifest(&data)?.hash));
    }
    let config = configuration(input, options, kind, Capability::Object, &aliases)?;
    Ok((config, dependencies))
}
