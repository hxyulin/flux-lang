//! Reuse of compiled host executables, without reusing their execution effects.
use std::path::{Path, PathBuf};

use flux_hir::flib::{Encode, Encoder};
use serde::{Deserialize, Serialize};

use crate::build::CompileRequest;
use crate::flib::freshness::Snapshot;

#[derive(Deserialize, Serialize)]
struct Record {
    version: u32,
    key: Vec<u8>,
    output: [u8; 32],
    snapshots: Vec<Vec<u8>>,
}

pub(crate) struct HostCache {
    key: Vec<u8>,
    native: crate::native_link::Snapshot,
}

fn path(output: &Path) -> PathBuf {
    let mut name = output.as_os_str().to_owned();
    name.push(".host-fresh.json");
    PathBuf::from(name)
}

pub(crate) fn invalidate(output: &Path) -> Result<(), String> {
    match std::fs::remove_file(path(output)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

impl HostCache {
    pub(crate) fn new(
        request: &CompileRequest<'_>,
        generated: Option<(&str, &str)>,
    ) -> Result<Option<Self>, String> {
        // Arbitrary host expansion effects and native link resolution do not
        // have an input contract here. Keep those compilations conservative.
        if request.sources.is_empty()
            || request.sources.iter().any(|s| s.host_macros.is_some())
            || request.native != flux_codegen::NativeLink::default()
        {
            return Ok(None);
        }
        let current = std::env::current_dir().map_err(|error| error.to_string())?;
        if request.file.to_str().is_none()
            || request.output.to_str().is_none()
            || current.to_str().is_none()
            || request.sources.iter().any(|source| {
                source.file.to_str().is_none()
                    || source
                        .compilation_dir
                        .as_ref()
                        .is_some_and(|path| path.to_str().is_none())
                    || source
                        .package_identity
                        .as_ref()
                        .is_some_and(|(path, _, _)| path.to_str().is_none())
            })
        {
            return Ok(None);
        }
        let native = match crate::native_link::Snapshot::capture(&request.native) {
            Ok(snapshot) => snapshot,
            Err(_) => return Ok(None),
        };
        let mut e = Encoder::new();
        macro_rules! field {
            ($value:expr) => {
                $value.encode(&mut e).map_err(|error| error.to_string())?
            };
        }
        field!("FLUX-HOST-v1");
        field!(request.file.to_path_buf());
        field!(request.crate_name);
        field!(request.output.to_path_buf());
        field!(current);
        field!(request.options.optimize);
        field!(request.options.fast_build);
        field!(crate::backend_units::count(request.options.fast_build)?.get());
        field!(request.options.debug_assertions());
        field!(
            flux_codegen::Target::host()
                .ok_or("unsupported host target")?
                .triple()
        );
        let builtin = crate::builtins::active()
            .filter(|suite| suite.use_object(request.options.optimized()))
            .and_then(|suite| suite.object.as_deref())
            .map(|object| flux_hir::flib::digest(object).to_vec());
        field!(builtin);
        field!(serde_json::to_vec(&native).map_err(|error| error.to_string())?);
        field!(request.sources.len());
        for source in request.sources {
            field!(source.file);
            field!(source.name);
            field!(source.edition);
            field!(source.compilation_dir);
            field!(source.package_identity);
            field!(source.dependencies);
            field!(source.cfg.names);
            field!(source.cfg.values);
            field!(source.environment);
        }
        field!(generated.is_some());
        if let Some((wrapper, original)) = generated {
            field!(wrapper);
            field!(original);
        }
        Ok(Some(Self {
            key: flux_hir::flib::digest(&e.into_bytes()).to_vec(),
            native,
        }))
    }

    pub(crate) fn is_fresh(&self, request: &CompileRequest<'_>) -> Result<bool, String> {
        let Ok(bytes) = std::fs::read(path(request.output)) else {
            return Ok(false);
        };
        let Ok(record) = serde_json::from_slice::<Record>(&bytes) else {
            return Ok(false);
        };
        if record.version != 1
            || record.key != self.key
            || record.snapshots.len() != request.sources.len()
            || crate::artifact_cache::hash_file(request.output).ok() != Some(record.output)
        {
            return Ok(false);
        }
        for (bytes, source) in record.snapshots.iter().zip(request.sources) {
            let Ok(snapshot) = Snapshot::from_bytes(bytes) else {
                return Ok(false);
            };
            if !snapshot.is_fresh(&self.key, &[], source.environment.as_ref())? {
                return Ok(false);
            }
        }
        Ok(self.native.is_fresh())
    }

    pub(crate) fn record(
        &self,
        request: &CompileRequest<'_>,
        items: &flux_hir::ItemTree,
        generated: Option<&str>,
        consumed: &flux_codegen::NativeLinkInputs,
    ) -> Result<(), String> {
        if !self.native.is_fresh() || !self.native.matches_consumed(consumed) {
            return Ok(());
        }
        // Source-level #[link] declarations also bypass the supported contract.
        if !items.native_libraries.is_empty() {
            return Ok(());
        }
        let mut snapshots = Vec::new();
        for source in request.sources {
            let mut matches = items.crates.iter().enumerate().filter(|(_, data)| {
                let module = &items.modules[data.root];
                data.root.into_raw() >= items.core.modules
                    && items
                        .files
                        .get(&module.file)
                        .is_some_and(|file| file.path == source.file)
            });
            let Some((index, _)) = matches.next() else {
                return Ok(());
            };
            if matches.next().is_some() {
                return Ok(());
            }
            let original = generated
                .filter(|_| source.file == request.file)
                .map(|text| (request.file, text));
            let Some(snapshot) = Snapshot::capture_generated(
                items,
                flux_hir::CrateId(index as u32),
                &self.key,
                original,
            )?
            else {
                return Ok(());
            };
            snapshots.push(snapshot.to_bytes()?);
        }
        let record = Record {
            version: 1,
            key: self.key.clone(),
            output: crate::artifact_cache::hash_file(request.output)?,
            snapshots,
        };
        crate::flib::write_atomic(
            &path(request.output),
            &serde_json::to_vec(&record).map_err(|e| e.to_string())?,
        )
        .map_err(|error| error.to_string())
    }
}
