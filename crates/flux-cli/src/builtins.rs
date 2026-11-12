//! Explicitly staged, compiler-owned built-in metadata for fresh CLI processes.

use flux_syntax::PanicStrategy;
use std::cell::Cell;
use std::io::{IsTerminal, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, digest};
use flux_hir::{ItemTree, SourceDb};
use flux_mir::FileMir;
use flux_ty::Signatures;

use crate::flib::executable::{CheckMethod, CheckedIdentity, ExecutableIdentity};
use crate::flib::{Archive, write_atomic};

const MAGIC: &[u8; 8] = b"FLUXSUIT";
const VERSION: u32 = 6;
const NAME: &str = "flux-builtins.flib";
const UNWIND_NAME: &str = "flux-builtins-unwind.flib";
static ABORT: OnceLock<Option<Arc<BuiltinSuite>>> = OnceLock::new();
static UNWIND: OnceLock<Option<Arc<BuiltinSuite>>> = OnceLock::new();
static SOURCE_SELECTED: AtomicBool = AtomicBool::new(false);
std::thread_local! { static STRATEGY: Cell<PanicStrategy> = const { Cell::new(PanicStrategy::Abort) }; }
fn slot(strategy: PanicStrategy) -> &'static OnceLock<Option<Arc<BuiltinSuite>>> {
    match strategy {
        PanicStrategy::Abort => &ABORT,
        PanicStrategy::Unwind => &UNWIND,
    }
}
fn suite_name(strategy: PanicStrategy) -> &'static str {
    match strategy {
        PanicStrategy::Abort => NAME,
        PanicStrategy::Unwind => UNWIND_NAME,
    }
}
pub(crate) fn selected_strategy() -> PanicStrategy {
    STRATEGY.with(Cell::get)
}
pub(crate) struct StrategyGuard(PanicStrategy);
impl Drop for StrategyGuard {
    fn drop(&mut self) {
        STRATEGY.with(|current| current.set(self.0));
    }
}
/// Nested host builds select their own MIR/object variant and restore the caller.
/// In-process callers which never selected a suite continue to use sources.
pub(crate) fn strategy_scope(strategy: PanicStrategy) -> Result<StrategyGuard, String> {
    if ABORT.get().is_some() || UNWIND.get().is_some() {
        initialize_for_strategy(strategy)?;
    }
    let previous = STRATEGY.with(|current| current.replace(strategy));
    Ok(StrategyGuard(previous))
}

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
    slot(selected_strategy()).get().and_then(Option::as_ref)
}

/// Selects one source or suite path before any compilation starts.
pub fn initialize() -> Result<(), String> {
    initialize_for_strategy(selected_strategy())
}

pub(crate) fn initialize_for_strategy(strategy: PanicStrategy) -> Result<(), String> {
    let active = slot(strategy);
    if active.get().is_some() {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = executable.with_file_name(suite_name(strategy));
    let trace = std::env::var_os("FLUX_BUILTINS_TRACE").is_some();
    let selected = if SOURCE_SELECTED.load(Ordering::Acquire)
        || std::env::var_os("FLUX_BUILTINS_SOURCE").is_some()
    {
        if trace {
            eprintln!("flux builtins: source (forced)");
        }
        None
    } else {
        let start = Instant::now();
        match load(&path, &executable)? {
            Some((suite, method)) => {
                if suite.mir.panic_strategy != strategy {
                    return Err(invalid(&path, "built-in panic strategy mismatch"));
                }
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
                } else if path.exists() && std::io::stderr().is_terminal() {
                    // Only interactive runs get the note: piped stderr, which
                    // tests compare, is unchanged in the slower source mode.
                    eprintln!(
                        "note: {} was staged by a different compiler; using library \
                         sources instead. Run `fluxc build-builtins` to restage it.",
                        path.display()
                    );
                }
                None
            }
        }
    };
    if selected.is_none() {
        // Source HIR caches cannot be mixed with source-forbidden staged caches
        // later in this process. Keep nested host builds in source mode too.
        if ABORT.get().and_then(Option::as_ref).is_some()
            || UNWIND.get().and_then(Option::as_ref).is_some()
        {
            return Err(format!(
                "{}: missing strategy-compatible builtins; stage this variant before mixing host and target builds",
                path.display()
            ));
        }
        SOURCE_SELECTED.store(true, Ordering::Release);
    }
    active
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
    let stamp = read_stamp(path);
    let (checked, current) = measured("identity", || {
        check_executable(&identity, stamp.as_ref(), executable)
    })
    .map_err(|e| invalid(path, e))?;
    let cached = stamp
        .as_ref()
        .is_some_and(|stamp| stamp.suite.matches_metadata(&metadata));
    if checked.hash != identity.hash() {
        // A stale suite: remember this executable's hash too, so later runs
        // fall back to source mode without hashing it again.
        if let (Some(stamp), Some(current)) = (stamp.filter(|_| cached), current) {
            let _ = write_stamp_record(path, &stamp.suite, Some(current));
        }
        return Ok(None);
    }
    measured("checksum", || {
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
    if !cached || current.is_some() {
        // Both files matched by content. Record their current stat fields so
        // the next process can skip hashing; a read-only install keeps working.
        let (suite, seen) = match stamp {
            Some(stamp) if cached => (stamp.suite, stamp.executable),
            _ => (
                ExecutableIdentity::from_metadata(&metadata, digest(&bytes)),
                None,
            ),
        };
        let _ = write_stamp_record(path, &suite, current.or(seen));
    }
    // Staging type checked these exact bytes with this compiler.
    let mut suite = decode_payload(payload, archive.object, true).map_err(|e| invalid(path, e))?;
    suite.compiler = checked.hash;
    Ok(Some((suite, checked.method)))
}

/// The `.flib.stamp` record: the suite file's stat fields and hash, and the
/// stat fields and hash of the executable that last loaded it.
/// The suite header records the executable's stat fields at staging, but those
/// change when the executable is copied, linked or has its metadata changed.
struct SuiteStamp {
    suite: ExecutableIdentity,
    executable: Option<ExecutableIdentity>,
}

fn read_stamp(path: &Path) -> Option<SuiteStamp> {
    let bytes = std::fs::read(path.with_extension("flib.stamp")).ok()?;
    let mut d = Decoder::new(&bytes);
    let suite = ExecutableIdentity::decode(&mut d).ok()?;
    // Stamps written before the executable field existed end here.
    let executable = if d.remaining() == 0 {
        None
    } else {
        Option::<ExecutableIdentity>::decode(&mut d).ok()?
    };
    d.finish().ok()?;
    Some(SuiteStamp { suite, executable })
}

fn write_stamp_record(
    path: &Path,
    suite: &ExecutableIdentity,
    executable: Option<ExecutableIdentity>,
) -> Result<(), String> {
    let mut record = Encoder::new();
    suite.encode(&mut record).map_err(|e| e.to_string())?;
    executable.encode(&mut record).map_err(|e| e.to_string())?;
    write_atomic(&path.with_extension("flib.stamp"), &record.into_bytes())
        .map_err(|e| invalid(path, e))
}

/// Identifies the running executable for comparison with the suite header.
/// Matching stat fields, from the header or from the stamp, skip the hash;
/// otherwise the executable is hashed and its new identity returned as well.
fn check_executable(
    identity: &ExecutableIdentity,
    stamp: Option<&SuiteStamp>,
    executable: &Path,
) -> Result<(CheckedIdentity, Option<ExecutableIdentity>), String> {
    let metadata = std::fs::metadata(executable)
        .map_err(|e| format!("cannot stat {}: {e}", executable.display()))?;
    let seen = stamp
        .and_then(|stamp| stamp.executable.as_ref())
        .filter(|seen| seen.matches_metadata(&metadata));
    if identity.matches_metadata(&metadata) || seen.is_some() {
        let checked = CheckedIdentity {
            hash: seen.unwrap_or(identity).hash(),
            method: CheckMethod::Stat,
        };
        return Ok((checked, None));
    }
    let current = ExecutableIdentity::capture(executable)?;
    let checked = CheckedIdentity {
        hash: current.hash(),
        method: CheckMethod::Hash,
    };
    Ok((checked, Some(current)))
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

fn already_staged(
    path: &Path,
    executable: &Path,
    native: bool,
    optimized: bool,
    strategy: PanicStrategy,
) -> Option<Vec<u8>> {
    let check = (|| -> Result<Option<Vec<u8>>, String> {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let archive = Archive::decode(&bytes).map_err(|e| e.to_string())?;
        if archive.object.is_some() != native {
            return Ok(None);
        }
        let (identity, payload, checksum) =
            envelope(&archive.metadata).map_err(|e| e.to_string())?;
        let current = identity.check(executable)?;
        if current.hash != identity.hash() || checksum.as_slice() != digest(payload) {
            return Ok(None);
        }
        let mut d = Decoder::new(payload);
        let target = String::decode(&mut d).map_err(|e| e.to_string())?;
        let flags = u8::decode(&mut d).map_err(|e| e.to_string())?;
        let tier = flags & 1 != 0;
        if flags & !3 != 0 || (flags & 2 != 0) != (strategy == PanicStrategy::Unwind) {
            return Ok(None);
        }
        let object_hash = Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
        if flux_codegen::Target::host().is_none_or(|h| h.triple() != target)
            || (native && tier != optimized)
        {
            return Ok(None);
        }
        let valid = match (&archive.object, object_hash) {
            (Some(bytes), Some(hash)) => hash.as_slice() == digest(bytes),
            (None, None) => true,
            _ => false,
        };
        Ok(valid.then_some(bytes))
    })();
    check.ok().flatten()
}

fn write_stamp(path: &Path, bytes: &[u8], executable: &Path) -> Result<(), String> {
    let stamp = ExecutableIdentity::capture(path)?;
    if stamp.hash() != digest(bytes) {
        return Err(invalid(
            path,
            "built-in suite changed while recording its stat fields",
        ));
    }
    write_stamp_record(path, &stamp, ExecutableIdentity::capture(executable).ok())
}

fn decode_payload(
    payload: &[u8],
    object: Option<Vec<u8>>,
    staged: bool,
) -> Result<BuiltinSuite, String> {
    let mut d = Decoder::new(payload);
    if staged {
        d.skip_staged_mir_typing();
    }
    let target = String::decode(&mut d).map_err(|e| e.to_string())?;
    if flux_codegen::Target::host().is_none_or(|host| host.triple() != target) {
        return Err("built-in suite target mismatch".into());
    }
    let flags = u8::decode(&mut d).map_err(|e| e.to_string())?;
    if flags & !3 != 0 {
        return Err("invalid built-in configuration flags".into());
    }
    let optimized = flags & 1 != 0;
    let strategy = if flags & 2 != 0 {
        PanicStrategy::Unwind
    } else {
        PanicStrategy::Abort
    };
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
    mir.panic_strategy = strategy;
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
        panic_strategy: selected_strategy(),
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
    stage_with_strategy(native_object, optimized, PanicStrategy::Abort)
}

pub(crate) fn stage_with_strategy(
    native_object: bool,
    optimized: bool,
    strategy: PanicStrategy,
) -> Result<PathBuf, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = executable.with_file_name(suite_name(strategy));
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Linux current_exe resolves symlinks. A local target directory may itself
    // point to a build cache, so compare its canonical path as well.
    let local_output = ["target", "out"].iter().any(|name| {
        repository
            .join(name)
            .canonicalize()
            .is_ok_and(|root| path.starts_with(root))
    });
    if !local_output
        && !path
            .components()
            .any(|part| part.as_os_str() == "target" || part.as_os_str() == "out")
    {
        return Err("build-builtins requires an executable under target/ or out/".into());
    }
    if let Some(bytes) = already_staged(&path, &executable, native_object, optimized, strategy) {
        // Restaging an unchanged suite repairs its stamp, so the next load
        // takes the stat path for both the suite and the executable.
        let suite = std::fs::metadata(&path).map_err(|e| invalid(&path, e))?;
        let current = std::fs::metadata(&executable).map_err(|e| invalid(&path, e))?;
        let fresh = read_stamp(&path).is_some_and(|stamp| {
            stamp.suite.matches_metadata(&suite)
                && stamp
                    .executable
                    .is_some_and(|seen| seen.matches_metadata(&current))
        });
        if !fresh {
            write_stamp(&path, &bytes, &executable)?;
        }
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
        crate::report_all(&db, &mut typed.diagnostics.clone());
        crate::report_all(&db, &mut hir.diagnostics.clone());
        return Err("built-in suite contains source or type diagnostics".into());
    }
    let mut mir =
        flux_mir::build_file_with_roots_and_panic_strategy(hir, &typed, &functions, strategy);
    if !mir.diagnostics.is_empty() || !mir.bugs.is_empty() {
        crate::report_all(&db, &mut mir.diagnostics);
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
            flux_codegen::compile_object_with_panic_strategy(
                &mut module,
                target,
                if optimized {
                    flux_codegen::OptimizationTier::Full
                } else {
                    flux_codegen::OptimizationTier::None
                },
                strategy,
            )
            .map_err(|e| e.to_string())?,
        )
    } else {
        None
    };
    let mut e = Encoder::new();
    target.triple().encode(&mut e).map_err(|e| e.to_string())?;
    (u8::from(optimized) | (u8::from(strategy == PanicStrategy::Unwind) << 1))
        .encode(&mut e)
        .map_err(|e| e.to_string())?;
    object
        .as_ref()
        .map(|bytes| digest(bytes).to_vec())
        .encode(&mut e)
        .map_err(|e| e.to_string())?;
    flux_hir::flib::write_builtin_suite(&hir.items, &db, &mut e).map_err(|e| e.to_string())?;
    typed.sigs.encode_flib(&mut e).map_err(|e| e.to_string())?;
    mir.encode_flib(&mut e).map_err(|e| e.to_string())?;
    let payload = e.into_bytes();
    // Loading skips MIR type checks, so check the exact bytes here, once.
    decode_payload(&payload, object.clone(), false)
        .map_err(|e| format!("staged built-in suite does not load: {e}"))?;
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
    write_stamp(&path, &bytes, &executable)?;
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
