//! Content snapshots used to decide whether a package artifact can be reused.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use flux_hir::flib::{CrateIndices, Decode, Decoder, Encode, Encoder, Error, digest};
use flux_hir::{CrateId, ItemTree};

const MAGIC: &[u8; 8] = b"FLUXFRSH";
const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Input {
    path: PathBuf,
    /// Missing candidates are recorded too: adding `m.rs` next to `m/mod.rs`
    /// changes module resolution even when previously loaded files are unchanged.
    state: Option<(PathBuf, [u8; 32])>,
}

/// Inputs observed while compiling one crate under an explicit configuration.
/// Request configuration bytes must include the root path, crate identity,
/// edition, cfg, target, target kind, and every semantic/codegen option.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    compiler: [u8; 32],
    configuration: Vec<u8>,
    compilation_directory: PathBuf,
    dependencies: Vec<[u8; 32]>,
    environment: Option<BTreeMap<String, String>>,
    inputs: Vec<Input>,
    observed_environment: BTreeMap<String, Option<String>>,
}

impl Snapshot {
    /// Captures checked physical sources, or declines reuse for untracked inputs.
    /// Call only after complete successful HIR lowering and checking. Includes
    /// and environment macros use the HIR input trace. Actual host invocations
    /// decline this pre-expansion path; importing unused macros does not.
    pub fn capture(
        items: &ItemTree,
        krate: CrateId,
        configuration: &[u8],
        dependencies: &[[u8; 32]],
    ) -> Result<Option<Self>, String> {
        Self::capture_with_indices(
            items,
            krate,
            &CrateIndices::collect(items, krate),
            configuration,
            dependencies,
        )
    }

    pub(crate) fn capture_with_indices(
        items: &ItemTree,
        krate: CrateId,
        indices: &CrateIndices,
        configuration: &[u8],
        dependencies: &[[u8; 32]],
    ) -> Result<Option<Self>, String> {
        Self::capture_checked_root(items, krate, indices, configuration, dependencies, None)
    }

    /// A generated wrapper is reusable only with its exact generated text in
    /// configuration and the original physical root checked against disk.
    pub(crate) fn capture_generated(
        items: &ItemTree,
        krate: CrateId,
        configuration: &[u8],
        original: Option<(&Path, &str)>,
    ) -> Result<Option<Self>, String> {
        Self::capture_checked_root(
            items,
            krate,
            &CrateIndices::collect(items, krate),
            configuration,
            &[],
            original,
        )
    }

    fn capture_checked_root(
        items: &ItemTree,
        krate: CrateId,
        indices: &CrateIndices,
        configuration: &[u8],
        dependencies: &[[u8; 32]],
        original: Option<(&Path, &str)>,
    ) -> Result<Option<Self>, String> {
        let data = items
            .crates
            .get(krate.0 as usize)
            .ok_or("invalid snapshot crate")?;
        let tracked = data.tracked_inputs();
        // Only actual host invocations have untracked execution effects.
        // Merely importing or reexporting a macro does not execute its code.
        // An empty token result still occupies an outer trace entry.
        if !tracked.macro_outputs.is_empty() {
            return Ok(None);
        }
        let current = std::env::current_dir()
            .map_err(|e| format!("cannot snapshot compilation directory: {e}"))?;
        let root = items.modules[data.root].file;
        let mut inputs = BTreeMap::new();
        for &id in &indices.files {
            let id = flux_hir::FileId(id);
            let file = items.files.get(&id).ok_or("missing snapshot source")?;
            if file.expansion_of.is_some() {
                continue;
            }
            let path = absolute(&current, &file.path);
            let Ok(bytes) = std::fs::read(&path) else {
                return Ok(None);
            };
            let expected = if id == root {
                original
                    .filter(|(original, _)| absolute(&current, original) == path)
                    .map(|(_, text)| text.as_bytes())
                    .unwrap_or_else(|| file.text().as_bytes())
            } else {
                file.text().as_bytes()
            };
            if bytes != expected {
                return Ok(None);
            }
            let Ok(resolved) = std::fs::canonicalize(&path) else {
                return Ok(None);
            };
            if path.to_str().is_none() || resolved.to_str().is_none() {
                return Ok(None);
            }
            inputs.insert(
                path.clone(),
                Input {
                    path: path.clone(),
                    state: Some((resolved, digest(&bytes))),
                },
            );
            if id != root
                && let Some(candidate) = alternative_module(&path)
            {
                match std::fs::symlink_metadata(&candidate) {
                    // Without module-lookup tracing we cannot distinguish an
                    // explicit path from an alternate created during checking.
                    Ok(_) => return Ok(None),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                        inputs.entry(candidate.clone()).or_insert(Input {
                            path: candidate,
                            state: None,
                        });
                    }
                    Err(_) => return Ok(None),
                }
            }
        }
        for (path, hash) in &tracked.files {
            let path = absolute(&current, path);
            let Ok(bytes) = std::fs::read(&path) else {
                return Ok(None);
            };
            if digest(&bytes) != *hash {
                return Ok(None);
            }
            let Ok(resolved) = std::fs::canonicalize(&path) else {
                return Ok(None);
            };
            if path.to_str().is_none() || resolved.to_str().is_none() {
                return Ok(None);
            }
            let input = Input {
                path: path.clone(),
                state: Some((resolved, *hash)),
            };
            if inputs.get(&path).is_some_and(|old| old != &input) {
                return Ok(None);
            }
            inputs.insert(path, input);
        }
        for (key, expected) in &tracked.environment {
            if read_environment(key, data.environment.as_deref())?.as_ref() != expected.as_ref() {
                return Ok(None);
            }
        }
        if inputs.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            compiler: super::compiler_identity()?,
            configuration: configuration.to_vec(),
            compilation_directory: current,
            dependencies: dependencies.to_vec(),
            environment: data.environment.as_ref().map(|e| e.as_ref().clone()),
            inputs: inputs.into_values().collect(),
            observed_environment: tracked.environment,
        }))
    }

    /// Compares current file contents, symlink resolution, configuration and dependencies.
    /// Unreadable or missing files make an artifact stale and leave source errors to rebuilding.
    pub fn is_fresh(
        &self,
        configuration: &[u8],
        dependencies: &[[u8; 32]],
        environment: Option<&BTreeMap<String, String>>,
    ) -> Result<bool, String> {
        if self.compilation_directory
            != std::env::current_dir()
                .map_err(|e| format!("cannot inspect compilation directory: {e}"))?
        {
            return Ok(false);
        }
        if self.compiler != super::compiler_identity()?
            || self.configuration != configuration
            || self.dependencies != dependencies
            || self.environment.as_ref() != environment
        {
            return Ok(false);
        }
        for (key, expected) in &self.observed_environment {
            if read_environment(key, environment)?.as_ref() != expected.as_ref() {
                return Ok(false);
            }
        }
        for input in &self.inputs {
            match &input.state {
                Some((resolved, hash)) => {
                    if std::fs::canonicalize(&input.path).as_ref().ok() != Some(resolved) {
                        return Ok(false);
                    }
                    let Ok(bytes) = std::fs::read(&input.path) else {
                        return Ok(false);
                    };
                    if digest(&bytes) != *hash {
                        return Ok(false);
                    }
                }
                None => match std::fs::symlink_metadata(&input.path) {
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    _ => return Ok(false),
                },
            }
        }
        Ok(true)
    }

    /// Encodes a snapshot for metadata section two.
    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let mut e = Encoder::new();
        self.encode(&mut e).map_err(|e| e.to_string())?;
        Ok(e.into_bytes())
    }

    /// Decodes an entire snapshot, rejecting unsupported schemas and extra bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let mut d = Decoder::new(bytes);
        let result = Self::decode(&mut d).map_err(|e| e.to_string())?;
        d.finish().map_err(|e| e.to_string())?;
        Ok(result)
    }
}

fn absolute(current: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_owned()
    } else {
        current.join(path)
    }
}

fn alternative_module(path: &Path) -> Option<PathBuf> {
    if path.file_name()? == "mod.rs" {
        let directory = path.parent()?;
        let name = directory.file_name()?;
        Some(directory.parent()?.join(name).with_extension("rs"))
    } else if path.extension().is_some_and(|e| e == "rs") {
        Some(path.with_extension("").join("mod.rs"))
    } else {
        None
    }
}

fn read_environment(
    key: &str,
    environment: Option<&BTreeMap<String, String>>,
) -> Result<Option<String>, String> {
    if let Some(environment) = environment {
        return Ok(environment.get(key).cloned());
    }
    match std::env::var(key) {
        Ok(value) => Ok(Some(value)),
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => {
            Err(format!("environment variable `{key}` is not utf-8"))
        }
    }
}

fn write_hash(hash: &[u8; 32], e: &mut Encoder) {
    e.write(hash);
}
fn read_hash(d: &mut Decoder<'_>) -> Result<[u8; 32], Error> {
    let mut hash = [0; 32];
    hash.copy_from_slice(d.read(32)?);
    Ok(hash)
}
impl Encode for Snapshot {
    fn encode(&self, e: &mut Encoder) -> Result<(), Error> {
        e.write(MAGIC);
        VERSION.encode(e)?;
        write_hash(&self.compiler, e);
        self.configuration.encode(e)?;
        self.compilation_directory.encode(e)?;
        self.dependencies.len().encode(e)?;
        for hash in &self.dependencies {
            write_hash(hash, e);
        }
        self.environment.is_some().encode(e)?;
        if let Some(environment) = &self.environment {
            environment.len().encode(e)?;
            for (key, value) in environment {
                key.encode(e)?;
                value.encode(e)?;
            }
        }
        self.observed_environment.len().encode(e)?;
        for (key, value) in &self.observed_environment {
            key.encode(e)?;
            value.encode(e)?;
        }
        self.inputs.len().encode(e)?;
        for input in &self.inputs {
            input.path.encode(e)?;
            input.state.is_some().encode(e)?;
            if let Some((resolved, hash)) = &input.state {
                resolved.encode(e)?;
                write_hash(hash, e);
            }
        }
        Ok(())
    }
}
impl Decode for Snapshot {
    fn decode(d: &mut Decoder<'_>) -> Result<Self, Error> {
        if d.read(8)? != MAGIC || u32::decode(d)? != VERSION {
            return Err(Error::new("unsupported freshness snapshot"));
        }
        let compiler = read_hash(d)?;
        let configuration = Vec::<u8>::decode(d)?;
        let compilation_directory = PathBuf::decode(d)?;
        if !compilation_directory.is_absolute() {
            return Err(Error::new("invalid snapshot compilation directory"));
        }
        let mut dependencies = Vec::new();
        for _ in 0..d.length()? {
            dependencies.push(read_hash(d)?);
        }
        let environment = if bool::decode(d)? {
            let mut env = BTreeMap::new();
            for _ in 0..d.length()? {
                let key = String::decode(d)?;
                let value = String::decode(d)?;
                if env.insert(key, value).is_some() {
                    return Err(Error::new("duplicate snapshot environment key"));
                }
            }
            Some(env)
        } else {
            None
        };
        let mut observed_environment = BTreeMap::new();
        for _ in 0..d.length()? {
            let key = String::decode(d)?;
            let value = Option::<String>::decode(d)?;
            if observed_environment.insert(key, value).is_some() {
                return Err(Error::new("duplicate observed environment key"));
            }
        }
        let mut inputs = Vec::new();
        let mut paths = BTreeSet::new();
        for _ in 0..d.length()? {
            let path = PathBuf::decode(d)?;
            if !path.is_absolute() || !paths.insert(path.clone()) {
                return Err(Error::new("invalid snapshot input path"));
            }
            let state = if bool::decode(d)? {
                let resolved = PathBuf::decode(d)?;
                if !resolved.is_absolute() {
                    return Err(Error::new("invalid snapshot source path"));
                }
                Some((resolved, read_hash(d)?))
            } else {
                None
            };
            inputs.push(Input { path, state });
        }
        if inputs.is_empty() {
            return Err(Error::new("snapshot has no source inputs"));
        }
        Ok(Self {
            compiler,
            configuration,
            compilation_directory,
            dependencies,
            environment,
            inputs,
            observed_environment,
        })
    }
}
