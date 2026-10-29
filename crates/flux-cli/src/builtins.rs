//! Explicitly staged, compiler-owned built-in metadata for fresh CLI processes.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, digest};
use flux_hir::{ItemTree, SourceDb};
use flux_mir::FileMir;
use flux_ty::Signatures;

use crate::flib::executable::{CheckMethod, ExecutableIdentity};
use crate::flib::{Archive, write_atomic};

const MAGIC: &[u8; 8] = b"FLUXSUIT";
const VERSION: u32 = 1;
const NAME: &str = "flux-builtins.flib";
static ACTIVE: OnceLock<Option<Arc<BuiltinSuite>>> = OnceLock::new();

/// One trusted, interleaved `core`, `alloc` and `std` prefix.
#[derive(Debug)]
pub struct BuiltinSuite {
    pub compiler: [u8; 32],
    pub db: SourceDb,
    pub items: ItemTree,
    pub sigs: Signatures,
    pub mir: Arc<FileMir>,
    pub object: Option<Vec<u8>>,
    pub optimized: bool,
}

/// The suite explicitly selected by the CLI. In-process callers stay in source mode.
pub fn active() -> Option<&'static Arc<BuiltinSuite>> {
    ACTIVE.get().and_then(Option::as_ref)
}

/// Selects one source or suite path before any compilation starts.
pub fn initialize() -> Result<(), String> {
    if ACTIVE.get().is_some() {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = executable.with_file_name(NAME);
    let trace = std::env::var_os("FLUX_BUILTINS_TRACE").is_some();
    let selected = if std::env::var_os("FLUX_BUILTINS_SOURCE").is_some() {
        if trace {
            eprintln!("flux builtins: source (forced)");
        }
        None
    } else {
        let start = Instant::now();
        match load(&path, &executable)? {
            Some((suite, method)) => {
                crate::flib::checked_compiler_identity(suite.compiler)?;
                flux_hir::Core::forbid_source_loading();
                if trace {
                    eprintln!(
                        "flux builtins: suite {} identity={method:?} load={:.3}ms",
                        path.display(),
                        start.elapsed().as_secs_f64() * 1000.0
                    );
                }
                Some(Arc::new(suite))
            }
            None => {
                if trace {
                    eprintln!("flux builtins: source (suite missing or compiler changed)");
                }
                None
            }
        }
    };
    ACTIVE
        .set(selected)
        .map_err(|_| "built-in mode was selected twice".to_owned())
}

fn invalid(path: &Path, message: impl std::fmt::Display) -> String {
    format!(
        "{}: {message}; run `flux build-builtins` again",
        path.display()
    )
}

fn measured<T>(phase: &str, f: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    let start = std::env::var_os("FLUX_BUILTINS_TRACE")
        .is_some()
        .then(Instant::now);
    let result = f();
    if let Some(start) = start {
        eprintln!(
            "flux builtins: phase={phase} elapsed={:.3}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    result
}

/// Loads matching metadata, or returns source fallback for a missing/stale suite.
pub fn load(path: &Path, executable: &Path) -> Result<Option<(BuiltinSuite, CheckMethod)>, String> {
    let start = Instant::now();
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(invalid(path, e)),
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| invalid(path, e))?;
    let metadata = file.metadata().map_err(|e| invalid(path, e))?;
    let archive = Archive::decode(&bytes).map_err(|e| invalid(path, e))?;
    if std::env::var_os("FLUX_BUILTINS_TRACE").is_some() {
        eprintln!(
            "flux builtins: phase=read-and-framing elapsed={:.3}ms",
            start.elapsed().as_secs_f64() * 1000.0
        );
    }
    let (identity, payload, checksum) =
        envelope(&archive.metadata).map_err(|e| invalid(path, e))?;
    let checked =
        measured("identity", || identity.check(executable)).map_err(|e| invalid(path, e))?;
    if checked.hash != identity.hash() {
        return Ok(None);
    }
    measured("checksum", || {
        let cached = cached_checksum(path, &metadata);
        if std::env::var_os("FLUX_BUILTINS_TRACE").is_some() {
            eprintln!(
                "flux builtins: checksum={}",
                if cached { "Stat" } else { "Hash" }
            );
        }
        if !cached && checksum.as_slice() != digest(payload) {
            return Err("built-in suite checksum mismatch".into());
        }
        Ok(())
    })
    .map_err(|e| invalid(path, e))?;
    let mut suite = decode_payload(payload, archive.object).map_err(|e| invalid(path, e))?;
    suite.compiler = checked.hash;
    Ok(Some((suite, checked.method)))
}

fn cached_checksum(path: &Path, metadata: &std::fs::Metadata) -> bool {
    let check = (|| {
        let bytes = std::fs::read(path.with_extension("flib.stamp")).ok()?;
        let mut d = Decoder::new(&bytes);
        let identity = ExecutableIdentity::decode(&mut d).ok()?;
        d.finish().ok()?;
        Some(identity.matches_metadata(metadata))
    })();
    check.unwrap_or(false)
}

fn envelope(
    metadata: &[u8],
) -> Result<(ExecutableIdentity, &[u8], Vec<u8>), flux_hir::flib::Error> {
    let mut header = Decoder::new(metadata);
    if header.read(8)? != MAGIC || u32::decode(&mut header)? != VERSION {
        return Err(flux_hir::flib::Error::new(
            "unsupported built-in suite header",
        ));
    }
    let identity = ExecutableIdentity::decode(&mut header)?;
    let length = usize::try_from(u64::decode(&mut header)?)
        .map_err(|_| flux_hir::flib::Error::new("built-in suite is too large"))?;
    if length > 512 * 1024 * 1024 {
        return Err(flux_hir::flib::Error::new("built-in suite is too large"));
    }
    let payload = header.read(length)?;
    let checksum = Vec::<u8>::decode(&mut header)?;
    header.finish()?;
    Ok((identity, payload, checksum))
}

fn already_staged(path: &Path, executable: &Path, native: bool, optimized: bool) -> bool {
    let check = (|| -> Result<bool, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let archive = Archive::decode(&bytes).map_err(|e| e.to_string())?;
        if archive.object.is_some() != native {
            return Ok(false);
        }
        let (identity, payload, checksum) =
            envelope(&archive.metadata).map_err(|e| e.to_string())?;
        let current = identity.check(executable)?;
        if current.hash != identity.hash() || checksum.as_slice() != digest(payload) {
            return Ok(false);
        }
        let mut d = Decoder::new(payload);
        let target = String::decode(&mut d).map_err(|e| e.to_string())?;
        let tier = bool::decode(&mut d).map_err(|e| e.to_string())?;
        let object_hash = Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
        if flux_codegen::Target::host().is_none_or(|h| h.triple() != target)
            || (native && tier != optimized)
        {
            return Ok(false);
        }
        Ok(match (&archive.object, object_hash) {
            (Some(bytes), Some(hash)) => hash.as_slice() == digest(bytes),
            (None, None) => true,
            _ => false,
        })
    })();
    check.unwrap_or(false)
}

fn decode_payload(payload: &[u8], object: Option<Vec<u8>>) -> Result<BuiltinSuite, String> {
    let mut d = Decoder::new(payload);
    let target = String::decode(&mut d).map_err(|e| e.to_string())?;
    if flux_codegen::Target::host().is_none_or(|host| host.triple() != target) {
        return Err("built-in suite target mismatch".into());
    }
    let optimized = bool::decode(&mut d).map_err(|e| e.to_string())?;
    let object_hash = Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
    match (&object, object_hash) {
        (Some(bytes), Some(hash)) if hash.as_slice() == digest(bytes) => {}
        (None, None) => {}
        _ => return Err("built-in object checksum mismatch".into()),
    }
    let (db, items) = measured("hir-and-sources", || {
        flux_hir::flib::read_builtin_suite(&mut d).map_err(|e| e.to_string())
    })?;
    let mut sigs = Signatures::default();
    measured("types-and-signatures", || {
        sigs.decode_flib_into(&mut d, &items)
            .map_err(|e| e.to_string())
    })?;
    let mut mir = empty_mir();
    measured("mir-and-values", || {
        mir.decode_flib_into(&mut d, &items, &sigs)
            .map_err(|e| e.to_string())
    })?;
    d.finish().map_err(|e| e.to_string())?;
    Ok(BuiltinSuite {
        compiler: [0; 32],
        db,
        items,
        sigs,
        mir: Arc::new(mir),
        object,
        optimized,
    })
}

/// An empty destination for decoded MIR or source-program imports.
pub fn empty_mir() -> FileMir {
    FileMir {
        bodies: Default::default(),
        closures: Default::default(),
        consts: Default::default(),
        const_values: Default::default(),
        discriminants: Default::default(),
        diagnostics: Vec::new(),
        bugs: Vec::new(),
    }
}

/// The same compiler-owned crate identities used by ordinary source compilation.
pub fn crate_hashes(items: &ItemTree, compiler: [u8; 32]) -> Vec<(flux_hir::CrateId, [u8; 32])> {
    items
        .crates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.root.into_raw() < items.core.modules)
        .map(|(index, c)| {
            let mut bytes = compiler.to_vec();
            bytes.extend_from_slice(c.name.as_str().as_bytes());
            (flux_hir::CrateId(index as u32), digest(&bytes))
        })
        .collect()
}

/// Identities for a source graph sharing an optional native built-in object.
pub fn graph_hashes(items: &ItemTree) -> Result<Vec<(flux_hir::CrateId, [u8; 32])>, String> {
    use crate::flib::semantic::{Dependency, crate_hash};
    use flux_hir::flib::CrateIndices;
    let mut dependencies: Vec<_> = crate_hashes(items, crate::flib::compiler_identity()?)
        .into_iter()
        .map(|(krate, hash)| Dependency {
            krate,
            hash,
            indices: CrateIndices::collect(items, krate),
        })
        .collect();
    for (index, data) in items.crates.iter().enumerate() {
        if data.root.into_raw() < items.core.modules {
            continue;
        }
        let krate = flux_hir::CrateId(index as u32);
        let hash = crate_hash(items, krate, &dependencies, &[])?;
        dependencies.push(Dependency {
            krate,
            hash,
            indices: CrateIndices::collect(items, krate),
        });
    }
    Ok(dependencies
        .into_iter()
        .map(|d| (d.krate, d.hash))
        .collect())
}

/// Eagerly checks and stages the compiler-owned suite beside this executable.
pub fn stage(native_object: bool, optimized: bool) -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = executable.with_file_name(NAME);
    if !path
        .components()
        .any(|part| part.as_os_str() == "target" || part.as_os_str() == "out")
    {
        return Err("build-builtins requires an executable under target/ or out/".into());
    }
    if already_staged(&path, &executable, native_object, optimized) {
        return Ok(path);
    }
    let identity = ExecutableIdentity::capture(&executable)?;
    crate::flib::checked_compiler_identity(identity.hash())?;
    let db = SourceDb::new();
    let hir = flux_hir::Core::get().hir();
    let functions: Vec<_> = hir
        .items
        .functions
        .iter()
        .filter(|(_, f)| f.has_body)
        .map(|(id, _)| id)
        .collect();
    let typed = flux_ty::check_file_all(hir);
    if !typed.diagnostics.is_empty()
        || !hir.diagnostics.is_empty()
        || !db.syntax_errors().is_empty()
    {
        return Err("built-in suite contains source or type diagnostics".into());
    }
    let mir = flux_mir::build_file_with_roots(hir, &typed, &functions);
    if !mir.diagnostics.is_empty() || !mir.bugs.is_empty() {
        return Err(format!(
            "built-in suite contains MIR diagnostics: {:?} {:?}",
            mir.diagnostics, mir.bugs
        ));
    }
    let target = flux_codegen::Target::host().ok_or("unsupported built-in target")?;
    let hashes = crate_hashes(&hir.items, identity.hash());
    let object = if native_object {
        let mut module = flux_codegen::lower_crate(
            hir,
            &typed,
            &mir,
            flux_codegen::Compilation {
                kind: flux_codegen::OutputKind::BuiltinSuite,
                crate_hashes: &hashes,
                imported: &[],
                required_functions: &[],
                required_instances: &[],
                builtin_object: false,
            },
        )
        .map_err(|e| format!("cannot lower built-in object: {e:?}"))?;
        Some(
            flux_codegen::compile_object(
                &mut module,
                target,
                if optimized {
                    flux_codegen::OptimizationTier::Full
                } else {
                    flux_codegen::OptimizationTier::None
                },
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let mut e = Encoder::new();
    target.triple().encode(&mut e).map_err(|e| e.to_string())?;
    optimized.encode(&mut e).map_err(|e| e.to_string())?;
    object
        .as_ref()
        .map(|bytes| digest(bytes).to_vec())
        .encode(&mut e)
        .map_err(|e| e.to_string())?;
    flux_hir::flib::write_builtin_suite(&hir.items, &db, &mut e).map_err(|e| e.to_string())?;
    typed.sigs.encode_flib(&mut e).map_err(|e| e.to_string())?;
    mir.encode_flib(&mut e).map_err(|e| e.to_string())?;
    let payload = e.into_bytes();
    let mut header = Encoder::new();
    header.write(MAGIC);
    VERSION.encode(&mut header).map_err(|e| e.to_string())?;
    identity.encode(&mut header).map_err(|e| e.to_string())?;
    (payload.len() as u64)
        .encode(&mut header)
        .map_err(|e| e.to_string())?;
    header.write(&payload);
    digest(&payload)
        .as_slice()
        .encode(&mut header)
        .map_err(|e| e.to_string())?;
    let bytes = Archive {
        metadata: header.into_bytes(),
        object,
    }
    .encode()
    .map_err(|e| e.to_string())?;
    write_atomic(&path, &bytes).map_err(|e| invalid(&path, e))?;
    let stamp = ExecutableIdentity::capture(&path)?;
    if stamp.hash() != digest(&bytes) {
        return Err(invalid(
            &path,
            "built-in suite changed while recording its stat fields",
        ));
    }
    let mut record = Encoder::new();
    stamp.encode(&mut record).map_err(|e| e.to_string())?;
    write_atomic(&path.with_extension("flib.stamp"), &record.into_bytes())
        .map_err(|e| invalid(&path, e))?;
    Ok(path)
}

impl BuiltinSuite {
    /// Creates one compilation's providers without parsing or inferring built-ins.
    pub fn providers(
        &self,
    ) -> Result<(flux_ty::flib::LoadedTypes, flux_mir::imported::ImportedMir), String> {
        let hashes = crate_hashes(&self.items, self.compiler);
        let loaded = flux_ty::flib::LoadedTypes::with_builtin_prefix(
            &self.items,
            self.sigs.fork_for_extension(),
            &hashes,
        )
        .map_err(|e| e.to_string())?;
        let imported = flux_mir::imported::ImportedMir::with_builtin_prefix(
            &self.items,
            empty_mir(),
            self.mir.clone(),
        )
        .map_err(|e| e.to_string())?;
        Ok((loaded, imported))
    }
    /// Uses a native suite object only for a matching program tier.
    pub fn use_object(&self, optimized: bool) -> bool {
        self.object.is_some()
            && self.optimized == optimized
            && std::env::var_os("FLUX_BUILTINS_MIR_ONLY").is_none()
    }
}
