//! Driver for explicit Fluxlib compilation.

use crate::build::SourceInput;
use crate::{BuildOptions, CrateType, report_all};
use flux_cli::flib::semantic::Capability;
use flux_cli::flib::semantic::{CheckedCrate, Dependency, crate_hash, write_checked};
use flux_hir::flib::{CrateIndices, digest};
use flux_hir::flib::{Encode, Encoder};
use flux_hir::{CrateId, SourceDb};
use std::path::Path;

pub(crate) fn library(
    file: &Path,
    output: &Path,
    name: &str,
    options: BuildOptions,
    metadata_only: bool,
) -> Result<bool, String> {
    let input = SourceInput::standalone(file, name);
    let mut db = SourceDb::new();
    let root = db.load(file)?;
    let hir = flux_hir::lower(&mut db, root, flux_intern::Symbol::intern(name));
    let mut errors = db.syntax_errors();
    errors.extend(flux_cli::identifiers::diagnostics(&db));
    errors.extend(hir.diagnostics.iter().cloned());
    let typed = flux_mir::check_file(&hir);
    errors.extend(typed.diagnostics.iter().cloned());
    if !errors.is_empty() {
        report_all(&db, &mut errors);
        return Ok(false);
    }
    let mir = flux_mir::build_file(&hir, &typed);
    errors.extend(mir.diagnostics.iter().cloned());
    if !errors.is_empty() {
        report_all(&db, &mut errors);
        return Ok(false);
    }
    let own = hir.items.main_crate();
    let compiler_identity = flux_cli::flib::compiler_identity()?;
    let dependencies: Vec<_> = hir
        .items
        .crates
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != own.0 as usize)
        .map(|(i, c)| {
            let krate = CrateId(i as u32);
            let mut key = compiler_identity.to_vec();
            key.extend_from_slice(c.name.as_str().as_bytes());
            Dependency {
                krate,
                hash: digest(&key),
                indices: CrateIndices::collect(&hir.items, krate),
            }
        })
        .collect();
    let target = flux_codegen::Target::host().ok_or("unsupported host target")?;
    let capability = if metadata_only {
        Capability::Metadata
    } else {
        Capability::Object
    };
    let configuration = configuration(&input, options, CrateType::Lib, capability, &[])?;
    let hash = crate_hash(&hir.items, own, &dependencies, &configuration)?;
    let hashes: Vec<_> = dependencies
        .iter()
        .map(|d| (d.krate, d.hash))
        .chain(std::iter::once((own, hash)))
        .collect();
    let object = if metadata_only {
        None
    } else {
        let mut module = flux_codegen::lower_crate(
            &hir,
            &typed,
            &mir,
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::Library(own),
                crate_hashes: &hashes,
                imported: &[],
                required_functions: &[],
            },
        )
        .map_err(|mut errors| {
            report_all(&db, &mut errors);
            "cannot lower library".to_owned()
        })?;
        let tier = if options.optimize {
            flux_codegen::OptimizationTier::Full
        } else {
            flux_codegen::OptimizationTier::None
        };
        Some(flux_codegen::compile_object(&mut module, target, tier).map_err(|e| e.to_string())?)
    };
    let dependency_hashes: Vec<_> = dependencies.iter().map(|d| d.hash).collect();
    let freshness = flux_cli::flib::freshness::Snapshot::capture(
        &hir.items,
        own,
        &configuration,
        &dependency_hashes,
    )?;
    let bytes = write_checked(CheckedCrate {
        items: &hir.items,
        sigs: &typed.sigs,
        mir: &mir,
        krate: own,
        hash,
        dependencies: &dependencies,
        object,
        freshness: freshness.as_ref(),
    })?;
    flux_cli::flib::write_atomic(output, &bytes)
        .map_err(|e| format!("cannot write {}: {e}", output.display()))?;
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
    encode(options.optimize.encode(&mut e))?;
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
