//! Assembly of checked semantic sections into a Fluxlib archive.

use flux_hir::flib::{CrateIndices, Decode, Decoder, Encode, Encoder, IdMap};
use flux_hir::{CrateId, ItemTree, SourceDb};
use flux_mir::FileMir;
use flux_ty::Signatures;

use super::{Archive, ArchiveIndex, Metadata, MetadataIndex, Section, compiler_identity};

/// Framing and checksums are checked once against immutable artifact bytes.
#[derive(Debug)]
struct ArtifactIndex {
    archive: ArchiveIndex,
    metadata: MetadataIndex,
}

impl ArtifactIndex {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let archive = super::time("archive decode", || ArchiveIndex::decode(bytes))
            .map_err(|e| e.to_string())?;
        let compiler = compiler_identity()?;
        let metadata = super::time("metadata envelope", || {
            MetadataIndex::decode(archive.metadata(bytes), compiler)
        })
        .map_err(|e| e.to_string())?;
        let index = Self { archive, metadata };
        super::time("linkage validation", || validate_linkage(index.view(bytes)))?;
        Ok(index)
    }

    fn view<'a>(&'a self, bytes: &'a [u8]) -> ArtifactView<'a> {
        ArtifactView { bytes, index: self }
    }
}

#[derive(Clone, Copy)]
struct ArtifactView<'a> {
    bytes: &'a [u8],
    index: &'a ArtifactIndex,
}

impl<'a> ArtifactView<'a> {
    fn section(self, tag: u32) -> Result<&'a [u8], String> {
        self.index
            .metadata
            .section(self.index.archive.metadata(self.bytes), tag)
            .ok_or_else(|| format!("missing flux metadata section {tag}"))
    }

    fn object(self) -> Option<&'a [u8]> {
        self.index.archive.object(self.bytes)
    }
}

/// An exact crate identity already present in a compilation.
#[derive(Debug, Clone)]
pub struct Dependency {
    pub krate: CrateId,
    pub hash: [u8; 32],
    pub indices: CrateIndices,
}

/// A decoded dependency's reusable object and identity.
#[derive(Debug)]
pub struct Loaded {
    pub dependency: Dependency,
    pub object: Option<Vec<u8>>,
    pub required_instances: Vec<flux_ty::Instance>,
}

/// Computes a crate identity from its source inputs and exact dependency identities.
pub fn crate_hash(
    items: &ItemTree,
    krate: CrateId,
    dependencies: &[Dependency],
    configuration: &[u8],
) -> Result<[u8; 32], String> {
    crate_hash_with_indices(
        items,
        krate,
        &CrateIndices::collect(items, krate),
        dependencies,
        configuration,
    )
}

pub(crate) fn crate_hash_with_indices(
    items: &ItemTree,
    krate: CrateId,
    indices: &CrateIndices,
    dependencies: &[Dependency],
    configuration: &[u8],
) -> Result<[u8; 32], String> {
    let mut e = Encoder::new();
    let encode = |result: Result<(), flux_hir::flib::Error>| result.map_err(|e| e.to_string());
    encode("FLIB-CRATE-ID-v2".encode(&mut e))?;
    encode(compiler_identity()?.as_slice().encode(&mut e))?;
    let data = items
        .crates
        .get(krate.0 as usize)
        .ok_or("invalid crate identity")?;
    encode(data.name.encode(&mut e))?;
    encode(data.edition.encode(&mut e))?;
    encode(configuration.encode(&mut e))?;
    encode(data.compilation_dir.encode(&mut e))?;
    encode(data.environment.as_deref().encode(&mut e))?;
    encode(data.cfg.as_deref().encode(&mut e))?;
    let mut sources = Vec::new();
    for &index in &indices.files {
        let file = items
            .files
            .get(&flux_hir::FileId(index))
            .ok_or("missing crate source")?;
        if file.expansion_of.is_none() {
            sources.push((&file.path, file.text()));
        }
    }
    encode(sources.encode(&mut e))?;
    // Observations are a hash input, not one bounded metadata record.
    // Digest the complete stream before encoding it in the crate identity.
    let observations = expansion_observations(items, krate, indices, dependencies)?;
    encode(
        flux_hir::flib::digest(&observations)
            .as_slice()
            .encode(&mut e),
    )?;
    let tracked = data.tracked_inputs();
    let files: Vec<_> = tracked
        .files
        .iter()
        .map(|(path, hash)| (path, hash.as_slice()))
        .collect();
    encode(files.encode(&mut e))?;
    encode(tracked.environment.encode(&mut e))?;
    let mut aliases = Vec::new();
    for (alias, krate) in &data.dependencies {
        let hash = dependencies
            .iter()
            .find(|d| d.krate == *krate)
            .ok_or("missing dependency identity")?
            .hash;
        aliases.push((alias.as_str(), hash.to_vec()));
    }
    aliases.sort();
    encode(aliases.encode(&mut e))?;
    let mut hashes: Vec<_> = dependencies.iter().map(|d| d.hash.to_vec()).collect();
    hashes.sort();
    encode(hashes.encode(&mut e))?;
    Ok(flux_hir::flib::digest(&e.into_bytes()))
}

// Host macros can observe undeclared inputs. Hash their actual tokens and
// hygiene, with process-local source and context IDs remapped canonically.
fn expansion_observations(
    items: &ItemTree,
    krate: CrateId,
    own: &CrateIndices,
    dependencies: &[Dependency],
) -> Result<Vec<u8>, String> {
    let mut ids = IdMap::strict();
    own.add_mapping(&mut ids, krate, 0)
        .map_err(|e| e.to_string())?;
    let mut dependencies: Vec<_> = dependencies.iter().collect();
    dependencies.sort_by_key(|d| d.hash);
    for (slot, dependency) in dependencies.iter().enumerate() {
        dependency
            .indices
            .add_mapping(&mut ids, dependency.krate, (slot + 1) as u32)
            .map_err(|e| e.to_string())?;
    }
    // The embedded macros are implicit imports. Compiler identity already
    // covers them, even for callers that supply no explicit dependencies.
    let mut implicit: Vec<_> = items
        .crates
        .iter()
        .enumerate()
        .filter(|(index, data)| {
            let id = CrateId(*index as u32);
            id != krate
                && data.root.into_raw() < items.core.modules
                && !dependencies.iter().any(|d| d.krate == id)
        })
        .collect();
    implicit.sort_by_key(|(_, data)| data.name.as_str());
    for (offset, (index, _)) in implicit.into_iter().enumerate() {
        let id = CrateId(index as u32);
        CrateIndices::collect(items, id)
            .add_mapping(&mut ids, id, (dependencies.len() + offset + 1) as u32)
            .map_err(|e| e.to_string())?;
    }
    let mut e = Encoder::with_ids(ids);
    for &raw in &own.files {
        let file = items
            .files
            .get(&flux_hir::FileId(raw))
            .ok_or("missing expansion source")?;
        if file.expansion_of.is_none() {
            continue;
        }
        file.text().encode(&mut e).map_err(|e| e.to_string())?;
        for token in file.parse.root().tokens() {
            token
                .lexical_kind()
                .encode(&mut e)
                .map_err(|e| e.to_string())?;
            token.edition().encode(&mut e).map_err(|e| e.to_string())?;
            token
                .tree()
                .token_origin(token.index())
                .encode(&mut e)
                .map_err(|e| e.to_string())?;
        }
    }
    items.crates[krate.0 as usize]
        .tracked_inputs()
        .macro_outputs
        .encode(&mut e)
        .map_err(|e| e.to_string())?;
    Ok(e.into_bytes())
}

/// One checked crate and the identities needed to encode it.
#[derive(Debug)]
pub struct CheckedCrate<'a> {
    pub items: &'a ItemTree,
    pub sigs: &'a Signatures,
    pub mir: &'a FileMir,
    pub krate: CrateId,
    pub hash: [u8; 32],
    pub dependencies: &'a [Dependency],
    pub object: Option<Vec<u8>>,
    pub required_instances: &'a [flux_ty::Instance],
    pub freshness: Option<&'a super::freshness::Snapshot>,
}

/// Writes already checked metadata. This function never checks or lowers source.
pub fn write_checked(checked: CheckedCrate<'_>) -> Result<Vec<u8>, String> {
    let indices = CrateIndices::collect(checked.items, checked.krate);
    write_checked_with_indices(checked, &indices)
}

pub(crate) fn write_checked_with_indices(
    checked: CheckedCrate<'_>,
    indices: &CrateIndices,
) -> Result<Vec<u8>, String> {
    let CheckedCrate {
        items,
        sigs,
        mir,
        krate,
        hash,
        dependencies,
        object,
        required_instances,
        freshness,
    } = checked;
    let mut ids = IdMap::strict();
    indices
        .add_mapping(&mut ids, krate, 0)
        .map_err(|e| e.to_string())?;
    for (i, dep) in dependencies.iter().enumerate() {
        dep.indices
            .add_mapping(&mut ids, dep.krate, (i + 1) as u32)
            .map_err(|e| e.to_string())?;
    }
    let mut identity = Encoder::new();
    items.crates[krate.0 as usize]
        .name
        .encode(&mut identity)
        .map_err(|e| e.to_string())?;
    hash.as_slice()
        .encode(&mut identity)
        .map_err(|e| e.to_string())?;
    let hashes: Vec<_> = dependencies.iter().map(|d| d.hash.to_vec()).collect();
    hashes.encode(&mut identity).map_err(|e| e.to_string())?;
    let mut names = Encoder::with_ids(ids.clone());
    super::time("HIR encode", || {
        flux_hir::flib::write_items_with_indices(items, krate, indices, &mut names)
    })
    .map_err(|e| e.to_string())?;
    let mut types = Encoder::with_ids(ids.clone());
    super::time("signature encode", || sigs.encode_flib(&mut types)).map_err(|e| e.to_string())?;
    validate_required_instances(items, required_instances)?;
    let mut requirements = Encoder::with_ids(ids.clone());
    required_instances
        .encode(&mut requirements)
        .map_err(|e| e.to_string())?;
    let mut bodies = Encoder::with_ids(ids);
    super::time("MIR encode", || mir.encode_flib(&mut bodies)).map_err(|e| e.to_string())?;
    let mut linkage = Encoder::new();
    let target = artifact_target()?;
    (
        2u32,
        target,
        object
            .as_ref()
            .map(|bytes| flux_hir::flib::digest(bytes).to_vec()),
    )
        .encode(&mut linkage)
        .map_err(|e| e.to_string())?;
    linkage
        .blob(&requirements.into_bytes())
        .map_err(|e| e.to_string())?;
    let mut freshness = freshness
        .map(super::freshness::Snapshot::to_bytes)
        .transpose()?;
    let sections = (1..=9)
        .map(|tag| Section {
            tag,
            bytes: match tag {
                1 => identity_bytes(&mut identity),
                2 => freshness.take().unwrap_or_default(),
                3 => identity_bytes(&mut names),
                5 => identity_bytes(&mut types),
                8 => identity_bytes(&mut bodies),
                9 => identity_bytes(&mut linkage),
                _ => Vec::new(),
            },
        })
        .collect();
    let metadata = Metadata {
        compiler: compiler_identity()?,
        sections,
    }
    .encode()
    .map_err(|e| e.to_string())?;
    Archive { metadata, object }
        .encode()
        .map_err(|e| e.to_string())
}
fn identity_bytes(e: &mut Encoder) -> Vec<u8> {
    std::mem::take(e).into_bytes()
}

/// Loads semantic records into caller-owned staging state. Discard staging on failure.
pub fn load_checked(
    bytes: &[u8],
    db: &mut SourceDb,
    items: &mut ItemTree,
    sigs: &mut Signatures,
    mir: &mut FileMir,
    dependencies: &[Dependency],
) -> Result<Loaded, String> {
    let index = ArtifactIndex::decode(bytes)?;
    load_checked_with_resolver(
        index.view(bytes),
        db,
        items,
        sigs,
        mir,
        dependencies,
        std::sync::Arc::new(crate::proc_macro::FlibHostResolver::default()),
    )
}

fn load_checked_with_resolver(
    artifact: ArtifactView<'_>,
    db: &mut SourceDb,
    items: &mut ItemTree,
    sigs: &mut Signatures,
    mir: &mut FileMir,
    dependencies: &[Dependency],
    resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
) -> Result<Loaded, String> {
    let section = |tag| artifact.section(tag);
    let mut identity = Decoder::new(section(1)?);
    let name = flux_intern::Symbol::decode(&mut identity).map_err(|e| e.to_string())?;
    let hash = decode_hash(&mut identity)?;
    let hashes = Vec::<Vec<u8>>::decode(&mut identity).map_err(|e| e.to_string())?;
    identity.finish().map_err(|e| e.to_string())?;
    let mut ids = IdMap::strict();
    for (i, hash) in hashes.iter().enumerate() {
        let hash: [u8; 32] = hash
            .as_slice()
            .try_into()
            .map_err(|_| "invalid dependency hash")?;
        let dep = dependencies
            .iter()
            .find(|d| d.hash == hash)
            .ok_or("flux dependency identity mismatch; rebuild the artifact")?;
        dep.indices
            .add_decode_mapping(&mut ids, dep.krate, (i + 1) as u32)
            .map_err(|e| e.to_string())?;
    }
    let mut names = Decoder::with_ids(section(3)?, ids);
    names.set_proc_macro_resolver(resolver);
    let (krate, indices) = super::time("HIR decode and append", || {
        flux_hir::flib::read_items(items, db, &mut names)
    })
    .map_err(|e| {
        names
            .proc_macro_error()
            .map_or_else(|| e.to_string(), str::to_owned)
    })?;
    names.finish().map_err(|e| e.to_string())?;
    if items.crates[krate.0 as usize].name != name {
        return Err("inconsistent flux crate identity".into());
    }
    let mut types = Decoder::with_ids(section(5)?, names.ids.clone());
    super::time("signature decode", || {
        sigs.decode_flib_into(&mut types, items)
    })
    .map_err(|e| e.to_string())?;
    types.finish().map_err(|e| e.to_string())?;
    let mut requirements = Decoder::with_ids(link_requirements(artifact)?, names.ids.clone());
    let required_instances =
        Vec::<flux_ty::Instance>::decode(&mut requirements).map_err(|e| e.to_string())?;
    requirements.finish().map_err(|e| e.to_string())?;
    validate_required_instances(items, &required_instances)?;
    let mut bodies = Decoder::with_ids(section(8)?, names.ids);
    super::time("MIR decode and validation", || {
        mir.decode_flib_into(&mut bodies, items, sigs)
    })
    .map_err(|e| e.to_string())?;
    bodies.finish().map_err(|e| e.to_string())?;
    for &raw in &indices.functions {
        let id = flux_hir::FunctionId::from_raw(raw);
        if items.functions[id].has_body && mir.bodies.get(id).is_none() {
            return Err("missing imported function MIR".into());
        }
    }
    for &raw in &indices.consts {
        let id = flux_hir::ConstId::from_raw(raw);
        if items.consts[id].has_body && mir.consts.get(id).is_none() {
            return Err("missing imported constant MIR".into());
        }
    }
    Ok(Loaded {
        dependency: Dependency {
            krate,
            hash,
            indices,
        },
        object: artifact.object().map(<[u8]>::to_vec),
        required_instances,
    })
}
fn decode_hash(d: &mut Decoder<'_>) -> Result<[u8; 32], String> {
    Vec::<u8>::decode(d)
        .map_err(|e| e.to_string())?
        .try_into()
        .map_err(|_| "invalid crate hash".into())
}

/// Artifact identities needed to resolve a dependency closure before decoding IR.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub hash: [u8; 32],
    pub dependencies: Vec<[u8; 32]>,
    pub linkable: bool,
}

/// Reads and validates the identity envelope without loading semantic records.
pub fn manifest(bytes: &[u8]) -> Result<Manifest, String> {
    let index = ArtifactIndex::decode(bytes)?;
    manifest_from_view(index.view(bytes))
}

fn manifest_from_view(artifact: ArtifactView<'_>) -> Result<Manifest, String> {
    let mut d = Decoder::new(artifact.section(1)?);
    let name = String::decode(&mut d).map_err(|e| e.to_string())?;
    let hash = decode_hash(&mut d)?;
    let dependencies = Vec::<Vec<u8>>::decode(&mut d)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|h| {
            h.try_into()
                .map_err(|_| "invalid dependency hash".to_owned())
        })
        .collect::<Result<_, _>>()?;
    d.finish().map_err(|e| e.to_string())?;
    let linkable = artifact.object().is_some();
    Ok(Manifest {
        name,
        hash,
        dependencies,
        linkable,
    })
}

/// A validated artifact in dependency order, ready for semantic loading.
#[derive(Debug)]
pub struct Resolved {
    pub path: std::path::PathBuf,
    bytes: Vec<u8>,
    index: ArtifactIndex,
    pub manifest: Manifest,
}

impl Resolved {
    fn new(path: std::path::PathBuf, bytes: Vec<u8>) -> Result<Self, String> {
        let index = ArtifactIndex::decode(&bytes)?;
        let manifest = manifest_from_view(index.view(&bytes))?;
        Ok(Self {
            path,
            bytes,
            index,
            manifest,
        })
    }

    /// The immutable bytes whose framing and checksums were validated.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Resolves exact dependency hashes among explicit paths and their sibling Fluxlibs.
pub fn resolve_paths(
    paths: &[std::path::PathBuf],
    known: &[Dependency],
) -> Result<Vec<Resolved>, String> {
    resolve_paths_inner(paths, known).map(|(artifacts, _)| artifacts)
}

fn resolve_paths_inner(
    paths: &[std::path::PathBuf],
    known: &[Dependency],
) -> Result<(Vec<Resolved>, Vec<[u8; 32]>), String> {
    let mut candidates = std::collections::BTreeMap::new();
    let mut explicit = std::collections::BTreeMap::new();
    let mut roots = Vec::new();
    let mut directories = std::collections::BTreeSet::new();
    for path in paths {
        if let Some(&hash) = explicit.get(path) {
            roots.push(hash);
            continue;
        }
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let artifact =
            Resolved::new(path.clone(), bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        roots.push(artifact.manifest.hash);
        explicit.insert(path.clone(), artifact.manifest.hash);
        candidates.insert(artifact.manifest.hash, artifact);
        directories.insert(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_owned(),
        );
    }
    let need_search =
        candidates.values().any(|artifact| {
            artifact.manifest.dependencies.iter().any(|hash| {
                !known.iter().any(|d| d.hash == *hash) && !candidates.contains_key(hash)
            })
        });
    for directory in directories.into_iter().filter(|_| need_search) {
        let mut siblings = Vec::new();
        for entry in std::fs::read_dir(&directory)
            .map_err(|e| format!("cannot search {}: {e}", directory.display()))?
        {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "flib") {
                siblings.push(path);
            }
        }
        siblings.sort();
        for path in siblings {
            if explicit.contains_key(&path) {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(artifact) = Resolved::new(path, bytes) else {
                continue;
            };
            candidates.entry(artifact.manifest.hash).or_insert(artifact);
        }
    }
    fn visit(
        hash: [u8; 32],
        candidates: &mut std::collections::BTreeMap<[u8; 32], Resolved>,
        known: &mut std::collections::BTreeSet<[u8; 32]>,
        active: &mut std::collections::BTreeSet<[u8; 32]>,
        out: &mut Vec<Resolved>,
    ) -> Result<(), String> {
        if known.contains(&hash) {
            return Ok(());
        }
        if !active.insert(hash) {
            return Err("cyclic fluxlib dependency".into());
        }
        let artifact = candidates.remove(&hash).ok_or(
            "missing fluxlib dependency; supply its artifact beside the importing library",
        )?;
        for &dependency in &artifact.manifest.dependencies {
            visit(dependency, candidates, known, active, out)?;
        }
        active.remove(&hash);
        known.insert(hash);
        out.push(artifact);
        Ok(())
    }
    let mut known = known.iter().map(|d| d.hash).collect();
    let mut active = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for &root in &roots {
        visit(root, &mut candidates, &mut known, &mut active, &mut out)?;
    }
    Ok((out, roots))
}

/// Dependency state assembled before lowering the current source crate.
#[derive(Debug)]
pub struct DependencyState {
    pub db: SourceDb,
    pub items: ItemTree,
    pub sigs: Signatures,
    pub mir: FileMir,
    pub dependencies: Vec<Dependency>,
    pub objects: Vec<(std::path::PathBuf, Vec<u8>)>,
    pub required_instances: Vec<flux_ty::Instance>,
    /// Exact identities in the order of the explicit paths passed to `load`.
    pub roots: Vec<[u8; 32]>,
}

impl DependencyState {
    /// Starts with the compiler-owned embedded libraries and their signature tables.
    pub fn new() -> Result<Self, String> {
        let (db, mut items, sigs) = if let Some(suite) = crate::builtins::active() {
            (
                suite.db.fork(),
                suite.items.clone(),
                suite.sigs.fork_for_extension(),
            )
        } else {
            (
                SourceDb::new(),
                flux_hir::Core::get().items.clone(),
                flux_ty::LibraryTypes::get().sigs.fork_for_extension(),
            )
        };
        items.core = items.counts();
        let compiler = compiler_identity()?;
        let dependencies = items
            .crates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let krate = CrateId(i as u32);
                let mut bytes = compiler.to_vec();
                bytes.extend_from_slice(c.name.as_str().as_bytes());
                Dependency {
                    krate,
                    hash: flux_hir::flib::digest(&bytes),
                    indices: CrateIndices::collect(&items, krate),
                }
            })
            .collect();
        Ok(Self {
            db,
            items,
            sigs,
            mir: FileMir {
                bodies: Default::default(),
                closures: Default::default(),
                consts: Default::default(),
                const_values: Default::default(),
                discriminants: Default::default(),
                diagnostics: Vec::new(),
                bugs: Vec::new(),
            },
            dependencies,
            objects: Vec::new(),
            required_instances: Vec::new(),
            roots: Vec::new(),
        })
    }

    /// Loads exact artifact dependencies. Failure consumes the unpublished staging state.
    pub fn load(mut self, paths: &[std::path::PathBuf]) -> Result<Self, String> {
        let resolver = std::sync::Arc::new(crate::proc_macro::FlibHostResolver::default());
        let (artifacts, roots) = super::time("dependency discovery", || {
            resolve_paths_inner(paths, &self.dependencies)
        })?;
        self.roots = roots;
        for artifact in artifacts {
            let loaded = load_checked_with_resolver(
                artifact.index.view(&artifact.bytes),
                &mut self.db,
                &mut self.items,
                &mut self.sigs,
                &mut self.mir,
                &self.dependencies,
                resolver.clone(),
            )
            .map_err(|e| format!("{}: {e}", artifact.path.display()))?;
            if loaded.dependency.hash != artifact.manifest.hash {
                return Err("inconsistent artifact identity".into());
            }
            self.dependencies.push(loaded.dependency);
            self.required_instances.extend(loaded.required_instances);
            if let Some(object) = loaded.object {
                self.objects.push((artifact.path, object));
            }
        }
        Ok(self)
    }
}

fn artifact_target() -> Result<String, String> {
    flux_codegen::Target::host()
        .map(|t| t.triple().to_owned())
        .ok_or_else(|| "unsupported flux artifact target".into())
}

/// The artifact capability needed by the requested compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capability {
    Metadata,
    Object,
}

/// Checks recorded package inputs without decoding dependency IR or parsing source.
pub fn is_fresh(
    bytes: &[u8],
    configuration: &[u8],
    dependencies: &[[u8; 32]],
    environment: Option<&std::collections::BTreeMap<String, String>>,
    capability: Capability,
) -> Result<bool, String> {
    let index = match ArtifactIndex::decode(bytes) {
        Ok(index) => index,
        Err(_) => return Ok(false),
    };
    let artifact = index.view(bytes);
    if capability == Capability::Object && artifact.object().is_none() {
        return Ok(false);
    }
    let section = artifact.section(2)?;
    if section.is_empty() {
        return Ok(false);
    }
    let snapshot = match super::freshness::Snapshot::from_bytes(section) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    snapshot.is_fresh(configuration, dependencies, environment)
}

fn validate_linkage(artifact: ArtifactView<'_>) -> Result<bool, String> {
    let mut d = Decoder::new(artifact.section(9)?);
    let abi = u32::decode(&mut d).map_err(|e| e.to_string())?;
    if abi != 2 {
        return Err(format!(
            "unsupported flux linkage ABI {abi}; expected 2; rebuild the artifact"
        ));
    }
    let target = String::decode(&mut d).map_err(|e| e.to_string())?;
    if target != artifact_target()? {
        return Err("flux artifact target mismatch; rebuild the artifact".into());
    }
    let checksum = Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
    d.blob().map_err(|e| e.to_string())?;
    d.finish().map_err(|e| e.to_string())?;
    match (checksum, artifact.object()) {
        (None, None) => Ok(false),
        (Some(checksum), Some(bytes)) if checksum.as_slice() == flux_hir::flib::digest(bytes) => {
            Ok(true)
        }
        _ => Err("flux artifact object checksum mismatch; rebuild the artifact".into()),
    }
}

fn link_requirements(artifact: ArtifactView<'_>) -> Result<&[u8], String> {
    // Three bounded fields precede the length-delimited structural instance stream.
    let mut d = Decoder::new(artifact.section(9)?);
    u32::decode(&mut d).map_err(|e| e.to_string())?;
    String::decode(&mut d).map_err(|e| e.to_string())?;
    Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
    let bytes = d.blob().map_err(|e| e.to_string())?;
    d.finish().map_err(|e| e.to_string())?;
    Ok(bytes)
}

fn validate_required_instances(
    items: &ItemTree,
    instances: &[flux_ty::Instance],
) -> Result<(), String> {
    for instance in instances {
        let function = items
            .functions
            .get(instance.function)
            .ok_or("invalid object requirement function")?;
        let owner_count = match function.owner {
            Some(flux_hir::Owner::Impl(id)) => items.impls[id].generics.len(),
            Some(flux_hir::Owner::Trait(id)) => 1 + items.traits[id].generics.len(),
            None => 0,
        };
        let count = function.generics.len() + owner_count;
        if instance.args.len() != count
            || instance
                .args
                .iter()
                .any(|ty| ty.has_params() || ty.has_infer() || ty.has_error())
        {
            return Err("invalid concrete object requirement".into());
        }
    }
    Ok(())
}
