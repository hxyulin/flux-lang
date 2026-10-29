//! Fluxlib host descriptors. Files remain external and are checked before execution.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, digest};
use flux_hir::{ProcMacroExpander, ProcMacroResolver};

use super::{Catalog, CatalogState, Host, Reply, SUPPORTED_COMMIT, Service, TIMEOUT, Task};

const MAGIC: &[u8] = b"FLUXPM01";
const LOCAL_PROTOCOL: &str = "FPM1";

#[derive(Debug, Clone)]
pub(super) struct FileIdentity {
    path: PathBuf,
    hash: [u8; 32],
}
impl FileIdentity {
    fn capture(path: &Path) -> Result<Self, String> {
        let path = path.canonicalize().map_err(|e| {
            format!(
                "cannot locate procedural macro host {}: {e}",
                path.display()
            )
        })?;
        let bytes = std::fs::read(&path)
            .map_err(|e| format!("cannot read procedural macro host {}: {e}", path.display()))?;
        Ok(Self {
            path,
            hash: digest(&bytes),
        })
    }
    fn verify(&self) -> Result<(), String> {
        let bytes = std::fs::read(&self.path).map_err(|e| {
            format!(
                "cannot read procedural macro host {}: {e}; rebuild artifact",
                self.path.display()
            )
        })?;
        if digest(&bytes) != self.hash {
            return Err(format!(
                "procedural macro host changed: {}; rebuild artifact",
                self.path.display()
            ));
        }
        Ok(())
    }
    fn encode(&self, e: &mut Encoder) -> Result<(), String> {
        self.path.encode(e).map_err(|e| e.to_string())?;
        self.hash.as_slice().encode(e).map_err(|e| e.to_string())
    }
    fn decode(d: &mut Decoder<'_>) -> Result<Self, String> {
        let path = PathBuf::decode(d).map_err(|e| e.to_string())?;
        if !path.is_absolute() {
            return Err("procedural macro host path must be absolute".into());
        }
        let hash = Vec::<u8>::decode(d)
            .map_err(|e| e.to_string())?
            .try_into()
            .map_err(|_| "invalid procedural macro host hash")?;
        Ok(Self { path, hash })
    }
}

#[derive(Debug, Clone)]
pub(super) enum HostIdentity {
    Local {
        executable: FileIdentity,
    },
    Rustc {
        server: FileIdentity,
        dylib: FileIdentity,
    },
}
impl HostIdentity {
    pub(super) fn local(path: &Path) -> Result<Self, String> {
        Ok(Self::Local {
            executable: FileIdentity::capture(path)?,
        })
    }
    pub(super) fn rustc(server: &Path, dylib: &Path) -> Result<Self, String> {
        Ok(Self::Rustc {
            server: FileIdentity::capture(server)?,
            dylib: FileIdentity::capture(dylib)?,
        })
    }
    pub(super) fn local_path(&self) -> &Path {
        match self {
            Self::Local { executable } => &executable.path,
            Self::Rustc { .. } => unreachable!("local host identity"),
        }
    }
    pub(super) fn rustc_paths(&self) -> (&Path, &Path) {
        match self {
            Self::Rustc { server, dylib } => (&server.path, &dylib.path),
            Self::Local { .. } => unreachable!("rustc host identity"),
        }
    }
    pub(super) fn verify(&self) -> Result<(), String> {
        match self {
            Self::Local { executable } => executable.verify(),
            Self::Rustc { server, dylib } => {
                server.verify()?;
                dylib.verify()
            }
        }
    }
    fn encode_identity(&self, e: &mut Encoder) -> Result<(), String> {
        MAGIC.encode(e).map_err(|e| e.to_string())?;
        std::env::consts::ARCH
            .encode(e)
            .map_err(|e| e.to_string())?;
        std::env::consts::OS.encode(e).map_err(|e| e.to_string())?;
        match self {
            Self::Local { executable } => {
                0u8.encode(e).map_err(|e| e.to_string())?;
                LOCAL_PROTOCOL.encode(e).map_err(|e| e.to_string())?;
                executable.encode(e)
            }
            Self::Rustc { server, dylib } => {
                1u8.encode(e).map_err(|e| e.to_string())?;
                SUPPORTED_COMMIT.encode(e).map_err(|e| e.to_string())?;
                server.encode(e)?;
                dylib.encode(e)
            }
        }
    }
    pub(super) fn descriptor(&self, name: &str) -> Result<Vec<u8>, String> {
        let mut e = Encoder::new();
        self.encode_identity(&mut e)?;
        name.encode(&mut e).map_err(|e| e.to_string())?;
        Ok(e.into_bytes())
    }
    fn decode(bytes: &[u8]) -> Result<(Self, String), String> {
        let mut d = Decoder::new(bytes);
        if Vec::<u8>::decode(&mut d).map_err(|e| e.to_string())? != MAGIC {
            return Err("invalid procedural macro host descriptor".into());
        }
        let arch = String::decode(&mut d).map_err(|e| e.to_string())?;
        let os = String::decode(&mut d).map_err(|e| e.to_string())?;
        if arch != std::env::consts::ARCH || os != std::env::consts::OS {
            return Err("procedural macro host platform mismatch".into());
        }
        let kind = u8::decode(&mut d).map_err(|e| e.to_string())?;
        let protocol = String::decode(&mut d).map_err(|e| e.to_string())?;
        let identity = match kind {
            0 if protocol == LOCAL_PROTOCOL => Self::Local {
                executable: FileIdentity::decode(&mut d)?,
            },
            1 if protocol == SUPPORTED_COMMIT => Self::Rustc {
                server: FileIdentity::decode(&mut d)?,
                dylib: FileIdentity::decode(&mut d)?,
            },
            _ => return Err("unsupported procedural macro host protocol".into()),
        };
        let name = String::decode(&mut d).map_err(|e| e.to_string())?;
        if name.is_empty() {
            return Err("empty procedural macro export name".into());
        }
        d.finish().map_err(|e| e.to_string())?;
        Ok((identity, name))
    }
}

/// One compilation graph's restored host catalogs. Exports share host state,
/// while independent resolvers never share a compiler process or environment.
#[derive(Default)]
pub struct FlibHostResolver {
    catalogs: Mutex<BTreeMap<Vec<u8>, Arc<Catalog>>>,
}
impl std::fmt::Debug for FlibHostResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlibHostResolver").finish_non_exhaustive()
    }
}
impl ProcMacroResolver for FlibHostResolver {
    fn resolve(&self, bytes: &[u8]) -> Result<Arc<dyn ProcMacroExpander>, String> {
        let (identity, name) = HostIdentity::decode(bytes)?;
        identity.verify()?;
        if let HostIdentity::Local { .. } = &identity {
            return Ok(super::local::restore_host(identity, name));
        }
        let (server, dylib) = identity.rustc_paths();
        let mut key = Encoder::new();
        identity.encode_identity(&mut key)?;
        let key = key.into_bytes();
        let mut catalogs = self.catalogs.lock().expect("proc-macro resolver poisoned");
        let catalog = match catalogs.get(&key) {
            Some(catalog) => catalog.clone(),
            None => {
                let catalog = Arc::new(Catalog {
                    server: server.to_owned(),
                    processes: Mutex::new(CatalogState {
                        idle: None,
                        crates: Default::default(),
                    }),
                });
                catalogs.insert(key, catalog.clone());
                catalog
            }
        };
        drop(catalogs);
        // Listing uses an idle process that the first invocation crate later
        // adopts. Keep the catalog lock while starting it so exports agree.
        let mut state = catalog
            .processes
            .lock()
            .expect("proc-macro catalog poisoned");
        let service = state
            .idle
            .get_or_insert_with(|| Service::start(server.to_owned(), TIMEOUT));
        let exports = match service.submit(Task::List(dylib.to_owned()))? {
            Reply::List(exports) => exports,
            _ => return Err("unexpected proc-macro export list".into()),
        };
        if !exports.iter().any(|(export, _)| export == &name) {
            return Err(format!(
                "procedural macro export `{name}` is not present in host"
            ));
        }
        identity.verify()?;
        drop(state);
        Ok(Arc::new(Host {
            catalog,
            dylib: dylib.to_owned(),
            name,
            identity,
        }))
    }
}
