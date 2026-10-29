//! Assembly of checked semantic sections into a Fluxlib archive.

use flux_hir::flib::{CrateIndices, Decode, Decoder, Encode, Encoder, IdMap};
use flux_hir::{CrateId, ItemTree, SourceDb};
use flux_mir::FileMir;
use flux_ty::Signatures;

use super::{Archive, Metadata, Section, compiler_identity};

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
    let mut e = Encoder::new();
    let encode = |result: Result<(), flux_hir::flib::Error>| result.map_err(|e| e.to_string());
    encode("FLIB-CRATE-ID-v1".encode(&mut e))?;
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
    for index in CrateIndices::collect(items, krate).files {
        let file = items
            .files
            .get(&flux_hir::FileId(index))
            .ok_or("missing crate source")?;
        if file.expansion_of.is_none() {
            sources.push((&file.path, file.text()));
        }
    }
    encode(sources.encode(&mut e))?;
    encode(expansion_observations(items, krate, dependencies)?.encode(&mut e))?;
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
    dependencies: &[Dependency],
) -> Result<Vec<u8>, String> {
    let own = CrateIndices::collect(items, krate);
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
    for raw in own.files {
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
    CrateIndices::collect(items, krate)
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
    flux_hir::flib::write_items(items, krate, &mut names).map_err(|e| e.to_string())?;
    let mut types = Encoder::with_ids(ids.clone());
    sigs.encode_flib(&mut types).map_err(|e| e.to_string())?;
    validate_required_instances(items, required_instances)?;
    let mut requirements = Encoder::with_ids(ids.clone());
    required_instances
        .encode(&mut requirements)
        .map_err(|e| e.to_string())?;
    let mut bodies = Encoder::with_ids(ids);
    mir.encode_flib(&mut bodies).map_err(|e| e.to_string())?;
    let mut linkage = Encoder::new();
    let target = artifact_target()?;
    (
        2u32,
        target,
        object
            .as_ref()
            .map(|bytes| flux_hir::flib::digest(bytes).to_vec()),
        requirements.into_bytes(),
    )
        .encode(&mut linkage)
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
    load_checked_with_resolver(
        bytes,
        db,
        items,
        sigs,
        mir,
        dependencies,
        std::sync::Arc::new(crate::proc_macro::FlibHostResolver::default()),
    )
}

fn load_checked_with_resolver(
    bytes: &[u8],
    db: &mut SourceDb,
    items: &mut ItemTree,
    sigs: &mut Signatures,
    mir: &mut FileMir,
    dependencies: &[Dependency],
    resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
) -> Result<Loaded, String> {
    let archive = Archive::decode(bytes).map_err(|e| e.to_string())?;
    let metadata =
        Metadata::decode(&archive.metadata, compiler_identity()?).map_err(|e| e.to_string())?;
    let section = |tag| {
        metadata
            .sections
            .iter()
            .find(|s| s.tag == tag)
            .map(|s| s.bytes.as_slice())
            .ok_or_else(|| format!("missing flux metadata section {tag}"))
    };
    validate_linkage(&metadata, archive.object.as_deref())?;
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
    let (krate, indices) = flux_hir::flib::read_items(items, db, &mut names).map_err(|e| {
        names
            .proc_macro_error()
            .map_or_else(|| e.to_string(), str::to_owned)
    })?;
    names.finish().map_err(|e| e.to_string())?;
    if items.crates[krate.0 as usize].name != name {
        return Err("inconsistent flux crate identity".into());
    }
    let mut types = Decoder::with_ids(section(5)?, names.ids.clone());
    sigs.decode_flib_into(&mut types, items)
        .map_err(|e| e.to_string())?;
    types.finish().map_err(|e| e.to_string())?;
    let mut requirements = Decoder::with_ids(link_requirements(&metadata)?, names.ids.clone());
    let required_instances =
        Vec::<flux_ty::Instance>::decode(&mut requirements).map_err(|e| e.to_string())?;
    requirements.finish().map_err(|e| e.to_string())?;
    validate_required_instances(items, &required_instances)?;
    let mut bodies = Decoder::with_ids(section(8)?, names.ids);
    mir.decode_flib_into(&mut bodies, items, sigs)
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
        object: archive.object,
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
    let archive = Archive::decode(bytes).map_err(|e| e.to_string())?;
    let metadata =
        Metadata::decode(&archive.metadata, compiler_identity()?).map_err(|e| e.to_string())?;
    let identity = metadata
        .sections
        .iter()
        .find(|s| s.tag == 1)
        .ok_or("missing flux identity section")?;
    let mut d = Decoder::new(&identity.bytes);
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
    let linkable = validate_linkage(&metadata, archive.object.as_deref())?;
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
    pub bytes: Vec<u8>,
    pub manifest: Manifest,
}

/// Resolves exact dependency hashes among explicit paths and their sibling Fluxlibs.
pub fn resolve_paths(
    paths: &[std::path::PathBuf],
    known: &[Dependency],
) -> Result<Vec<Resolved>, String> {
    let mut candidates = std::collections::BTreeMap::new();
    let mut roots = Vec::new();
    let mut directories = std::collections::BTreeSet::new();
    for path in paths {
        let bytes =
            std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let info = manifest(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        roots.push(info.hash);
        candidates.insert(
            info.hash,
            Resolved {
                path: path.clone(),
                bytes,
                manifest: info,
            },
        );
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
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(info) = manifest(&bytes) else { continue };
            candidates.entry(info.hash).or_insert(Resolved {
                path,
                bytes,
                manifest: info,
            });
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
    for root in roots {
        visit(root, &mut candidates, &mut known, &mut active, &mut out)?;
    }
    Ok(out)
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
}

impl DependencyState {
    /// Starts with the compiler-owned embedded libraries and their signature tables.
    pub fn new() -> Result<Self, String> {
        let db = SourceDb::new();
        let mut items = flux_hir::Core::get().items.clone();
        items.core = items.counts();
        let sigs = flux_ty::LibraryTypes::get().sigs.fork_for_extension();
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
        })
    }

    /// Loads exact artifact dependencies. Failure consumes the unpublished staging state.
    pub fn load(mut self, paths: &[std::path::PathBuf]) -> Result<Self, String> {
        let resolver = std::sync::Arc::new(crate::proc_macro::FlibHostResolver::default());
        for artifact in resolve_paths(paths, &self.dependencies)? {
            let loaded = load_checked_with_resolver(
                &artifact.bytes,
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
    let archive = match Archive::decode(bytes) {
        Ok(a) => a,
        Err(_) => return Ok(false),
    };
    if capability == Capability::Object && archive.object.is_none() {
        return Ok(false);
    }
    let metadata = match Metadata::decode(&archive.metadata, compiler_identity()?) {
        Ok(m) => m,
        Err(_) => return Ok(false),
    };
    if validate_linkage(&metadata, archive.object.as_deref()).is_err() {
        return Ok(false);
    }
    let Some(section) = metadata.sections.iter().find(|s| s.tag == 2) else {
        return Ok(false);
    };
    if section.bytes.is_empty() {
        return Ok(false);
    }
    let snapshot = match super::freshness::Snapshot::from_bytes(&section.bytes) {
        Ok(s) => s,
        Err(_) => return Ok(false),
    };
    snapshot.is_fresh(configuration, dependencies, environment)
}

fn validate_linkage(metadata: &Metadata, object: Option<&[u8]>) -> Result<bool, String> {
    let section = metadata
        .sections
        .iter()
        .find(|s| s.tag == 9)
        .ok_or("missing flux link requirements")?;
    let mut d = Decoder::new(&section.bytes);
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
    Vec::<u8>::decode(&mut d).map_err(|e| e.to_string())?;
    d.finish().map_err(|e| e.to_string())?;
    match (checksum, object) {
        (None, None) => Ok(false),
        (Some(checksum), Some(bytes)) if checksum.as_slice() == flux_hir::flib::digest(bytes) => {
            Ok(true)
        }
        _ => Err("flux artifact object checksum mismatch; rebuild the artifact".into()),
    }
}

fn link_requirements(metadata: &Metadata) -> Result<&[u8], String> {
    let section = metadata
        .sections
        .iter()
        .find(|s| s.tag == 9)
        .ok_or("missing flux link requirements")?;
    // Three bounded fields precede the length-delimited structural instance stream.
    let mut d = Decoder::new(&section.bytes);
    u32::decode(&mut d).map_err(|e| e.to_string())?;
    String::decode(&mut d).map_err(|e| e.to_string())?;
    Option::<Vec<u8>>::decode(&mut d).map_err(|e| e.to_string())?;
    let length = d.length().map_err(|e| e.to_string())?;
    let bytes = d.read(length).map_err(|e| e.to_string())?;
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
