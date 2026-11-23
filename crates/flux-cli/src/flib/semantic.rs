//! Assembly of checked semantic sections into a Fluxlib archive.

mod fresh_record;

use flux_hir::flib::{
    CrateIndices, Decode, DecodedItems, Decoder, Encode, Encoder, IdMap, ItemArchiveLayout,
    PreparedItems,
};
use flux_hir::{CrateId, ItemTree, SourceDb};
use flux_mir::FileMir;
use flux_ty::Signatures;
use flux_ty::flib::DecodedSignatureRows;

use super::{Archive, ArchiveIndex, Metadata, MetadataIndex, Section, compiler_identity};

/// Framing and checksums are checked once against immutable artifact bytes.
#[derive(Debug)]
struct ArtifactIndex {
    archive: ArchiveIndex,
    metadata: MetadataIndex,
    read: ArchiveRead,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArchiveRead {
    Full,
    Metadata,
}

impl ArtifactIndex {
    fn decode(bytes: &[u8]) -> Result<Self, String> {
        let archive = super::time("archive decode", || ArchiveIndex::decode(bytes))
            .map_err(|e| e.to_string())?;
        Self::from_framing(archive, bytes, ArchiveRead::Full)
    }

    fn from_framing(
        archive: ArchiveIndex,
        bytes: &[u8],
        read: ArchiveRead,
    ) -> Result<Self, String> {
        let compiler = compiler_identity()?;
        let metadata_bytes = match read {
            ArchiveRead::Full => archive.metadata(bytes),
            ArchiveRead::Metadata => bytes,
        };
        let metadata = super::time("metadata envelope", || {
            MetadataIndex::decode(metadata_bytes, compiler)
        })
        .map_err(|e| e.to_string())?;
        let index = Self {
            archive,
            metadata,
            read,
        };
        super::time("linkage validation", || validate_linkage(index.view(bytes)))?;
        Ok(index)
    }

    fn metadata_start(&self) -> usize {
        match self.read {
            ArchiveRead::Full => self.archive.metadata.start,
            ArchiveRead::Metadata => 0,
        }
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
    fn metadata_bytes(self) -> &'a [u8] {
        match self.index.read {
            ArchiveRead::Full => self.index.archive.metadata(self.bytes),
            ArchiveRead::Metadata => self.bytes,
        }
    }

    fn section(self, tag: u32) -> Result<&'a [u8], String> {
        self.index
            .metadata
            .section(self.metadata_bytes(), tag)
            .ok_or_else(|| format!("missing flux metadata section {tag}"))
    }

    fn object(self) -> Option<&'a [u8]> {
        (self.index.read == ArchiveRead::Full)
            .then(|| self.index.archive.object(self.bytes))
            .flatten()
    }

    fn has_object(self) -> bool {
        self.index.archive.has_object()
    }
}

/// An exact crate identity already present in a compilation.
#[derive(Debug, Clone, PartialEq, Eq)]
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
    data.with_tracked_inputs(|tracked| {
        let observations =
            expansion_observations(items, krate, indices, dependencies, &tracked.macro_outputs)?;
        encode(crate::flib::digest(&observations).as_slice().encode(&mut e))?;
        let files: Vec<_> = tracked
            .files
            .iter()
            .map(|(path, hash)| (path, hash.as_slice()))
            .collect();
        encode(files.encode(&mut e))?;
        encode(tracked.environment.encode(&mut e))
    })?;
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
    Ok(crate::flib::digest(&e.into_bytes()))
}

// Host macros can observe undeclared inputs. Hash their actual tokens and
// hygiene, with process-local source and context IDs remapped canonically.
fn expansion_observations(
    items: &ItemTree,
    krate: CrateId,
    own: &CrateIndices,
    dependencies: &[Dependency],
    macro_outputs: &[Vec<flux_hir::ProcMacroToken>],
) -> Result<Vec<u8>, String> {
    let mut ids = IdMap::strict();
    own.add_origin_mapping(&mut ids, krate, 0)
        .map_err(|e| e.to_string())?;
    let mut dependencies: Vec<_> = dependencies.iter().collect();
    dependencies.sort_by_key(|d| d.hash);
    for (slot, dependency) in dependencies.iter().enumerate() {
        dependency
            .indices
            .add_origin_mapping(&mut ids, dependency.krate, (slot + 1) as u32)
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
            .add_origin_mapping(&mut ids, id, (dependencies.len() + offset + 1) as u32)
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
    macro_outputs.encode(&mut e).map_err(|e| e.to_string())?;
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
    write_checked_with_indices(checked, &indices, &[], &std::cell::OnceCell::new())
}

pub(crate) fn write_checked_with_indices(
    checked: CheckedCrate<'_>,
    indices: &CrateIndices,
    checked_dependencies: &[u8],
    function_summary: &std::cell::OnceCell<Option<flux_mir::imported::FunctionSummary>>,
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
    let mut names = Encoder::with_ids(ids);
    super::time("HIR encode", || {
        flux_hir::flib::write_items_with_indices(items, krate, indices, &mut names)
    })
    .map_err(|e| e.to_string())?;
    let mut types = Encoder::with_ids(std::mem::take(&mut names.ids));
    super::time("signature encode", || sigs.encode_flib(&mut types)).map_err(|e| e.to_string())?;
    validate_required_instances(items, required_instances)?;
    let mut requirements = Encoder::with_ids(std::mem::take(&mut types.ids));
    required_instances
        .encode(&mut requirements)
        .map_err(|e| e.to_string())?;
    let mut bodies = Encoder::with_ids(std::mem::take(&mut requirements.ids));
    super::time("MIR encode", || {
        if mir
            .encode_crate_flib(&mut bodies, true, items, sigs)
            .is_err()
        {
            // Restarted allocation tables can require more space than a
            // single stream when functions share a large constant.
            bodies = Encoder::with_ids(std::mem::take(&mut bodies.ids));
            mir.encode_crate_flib(&mut bodies, false, items, sigs)?;
        }
        Ok::<_, flux_hir::flib::Error>(())
    })
    .map_err(|e| e.to_string())?;
    let summary = super::time("function summary encode", || {
        let mut encoder = Encoder::with_ids(std::mem::take(&mut bodies.ids));
        if let Some(summary) = function_summary.get_or_init(|| {
            flux_mir::imported::FunctionSummary::collect(mir, items, &indices.functions)
        }) && summary.encode(&mut encoder).is_ok()
        {
            encoder.into_bytes()
        } else {
            Vec::new()
        }
    });
    let mut summary = Some(summary);
    let mut linkage = Encoder::new();
    let target = artifact_target()?;
    (
        2u32,
        target,
        object
            .as_ref()
            .map(|bytes| crate::flib::digest(bytes).to_vec()),
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
                4 => checked_dependencies.to_vec(),
                5 => identity_bytes(&mut types),
                6 => summary.take().unwrap_or_default(),
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
        false,
    )
}

#[allow(clippy::too_many_arguments)]
fn load_checked_with_resolver(
    artifact: ArtifactView<'_>,
    db: &mut SourceDb,
    items: &mut ItemTree,
    sigs: &mut Signatures,
    mir: &mut FileMir,
    dependencies: &[Dependency],
    resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
    append_indexes: bool,
) -> Result<Loaded, String> {
    let (dependency, ids) = load_hir(artifact, db, items, dependencies, resolver)?;
    let ir = load_ir(
        artifact,
        items,
        sigs,
        mir,
        &dependency,
        ids,
        (
            if append_indexes {
                SignatureInput::Append(None)
            } else {
                SignatureInput::Standalone
            },
            None,
        ),
    )?;
    Ok(Loaded {
        dependency,
        object: artifact.object().map(<[u8]>::to_vec),
        required_instances: ir.required_instances,
    })
}

fn load_hir(
    artifact: ArtifactView<'_>,
    db: &mut SourceDb,
    items: &mut ItemTree,
    dependencies: &[Dependency],
    resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
) -> Result<(Dependency, IdMap), String> {
    let dependency = |hash: &[u8; 32]| {
        dependencies
            .iter()
            .find(|dep| &dep.hash == hash)
            .map(|dep| (dep.krate, &dep.indices))
    };
    super::time("HIR decode and append", || {
        prepare_hir(
            artifact,
            dependency,
            resolver,
            &mut ItemArchiveLayout::new(items),
            db,
        )?
        .decode()?
        .append(items, db)
    })
}

fn prepare_hir<'a, 'dep>(
    artifact: ArtifactView<'a>,
    dependency: impl Fn(&[u8; 32]) -> Option<(CrateId, &'dep CrateIndices)>,
    resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
    layout: &mut ItemArchiveLayout,
    db: &mut SourceDb,
) -> Result<PreparedHir<'a>, String> {
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
        let (krate, indices) =
            dependency(&hash).ok_or("flux dependency identity mismatch; rebuild the artifact")?;
        indices
            .add_decode_mapping(&mut ids, krate, (i + 1) as u32)
            .map_err(|e| e.to_string())?;
    }
    let mut names = Decoder::with_ids(section(3)?, ids);
    names.set_proc_macro_resolver(resolver);
    let prepared = layout.prepare(db, &mut names).map_err(|e| e.to_string())?;
    Ok(PreparedHir {
        name,
        hash,
        names,
        prepared,
    })
}

struct PreparedHir<'a> {
    name: flux_intern::Symbol,
    hash: [u8; 32],
    names: Decoder<'a>,
    prepared: PreparedItems,
}

struct DecodedHir<'a> {
    header: (flux_intern::Symbol, [u8; 32]),
    names: Decoder<'a>,
    records: DecodedItems,
}

impl<'a> PreparedHir<'a> {
    fn decode(mut self) -> Result<DecodedHir<'a>, String> {
        let records = self.prepared.decode(&mut self.names).map_err(|e| {
            self.names
                .proc_macro_error()
                .map_or_else(|| e.to_string(), str::to_owned)
        })?;
        Ok(DecodedHir {
            header: (self.name, self.hash),
            names: self.names,
            records,
        })
    }
}

impl DecodedHir<'_> {
    fn append(
        self,
        items: &mut ItemTree,
        db: &mut SourceDb,
    ) -> Result<(Dependency, IdMap), String> {
        let (krate, indices) = self
            .records
            .append(items, db, &self.names.ids)
            .map_err(|e| e.to_string())?;
        self.names.finish().map_err(|e| e.to_string())?;
        let (name, hash) = self.header;
        if items.crates[krate.0 as usize].name != name {
            return Err("inconsistent flux crate identity".into());
        }
        Ok((
            Dependency {
                krate,
                hash,
                indices,
            },
            self.names.ids,
        ))
    }
}

struct LoadedIr {
    required_instances: Vec<flux_ty::Instance>,
    function_summary: Option<flux_mir::imported::FunctionSummary>,
}

enum SignatureInput {
    Standalone,
    Append(Option<Result<DecodedSignatureRows, String>>),
}

fn load_ir(
    artifact: ArtifactView<'_>,
    items: &ItemTree,
    sigs: &mut Signatures,
    mir: &mut FileMir,
    dependency: &Dependency,
    ids: IdMap,
    (signature_input, defer): (SignatureInput, Option<&Resolved>),
) -> Result<LoadedIr, String> {
    let ids = match signature_input {
        SignatureInput::Append(Some(rows)) => {
            super::time("signature append", || {
                rows.map(|rows| rows.append(sigs, items))
            })?;
            ids
        }
        source => {
            let mut types = Decoder::with_ids(artifact.section(5)?, ids);
            super::time("signature decode", || match source {
                SignatureInput::Standalone => sigs.decode_flib_into(&mut types, items),
                SignatureInput::Append(_) => sigs.decode_appended_flib_into(&mut types, items),
            })
            .map_err(|e| e.to_string())?;
            types.finish().map_err(|e| e.to_string())?;
            types.ids
        }
    };
    let mut requirements = Decoder::with_ids(link_requirements(artifact)?, ids);
    let required_instances =
        Vec::<flux_ty::Instance>::decode(&mut requirements).map_err(|e| e.to_string())?;
    requirements.finish().map_err(|e| e.to_string())?;
    validate_required_instances(items, &required_instances)?;
    let records = artifact.section(6)?;
    let source = defer
        .filter(|_| !records.is_empty() && artifact.has_object())
        .map(|source| {
            let section = artifact
                .index
                .metadata
                .sections
                .iter()
                .find(|section| section.tag == 8)
                .expect("validated metadata has a MIR section");
            flux_mir::flib::DeferredFunctionSource {
                bytes: source.bytes.clone(),
                section_end: artifact.index.metadata_start() + section.bytes.end,
                functions: &dependency.indices.functions,
            }
        });
    let mut bodies = Decoder::with_ids(artifact.section(8)?, requirements.ids);
    super::time("MIR decode and validation", || {
        mir.decode_flib_for_import(
            &mut bodies,
            items,
            sigs,
            crate::backend_workers::limit().get(),
            source,
        )
    })
    .map_err(|e| e.to_string())?;
    bodies.finish().map_err(|e| e.to_string())?;
    for &raw in &dependency.indices.functions {
        let id = flux_hir::FunctionId::from_raw(raw);
        if items.functions[id].has_body && !mir.has_function_body(id) {
            return Err("missing imported function MIR".into());
        }
    }
    for &raw in &dependency.indices.consts {
        let id = flux_hir::ConstId::from_raw(raw);
        if items.consts[id].has_body
            && mir.consts.get(id).is_none()
            && !mir.has_exported_data_value(items, sigs, id)
        {
            return Err("missing imported constant MIR".into());
        }
    }
    let function_summary = if records.is_empty() {
        None
    } else {
        let mut summary = Decoder::with_ids(records, bodies.ids);
        let facts = flux_mir::imported::FunctionSummary::decode(
            &mut summary,
            items,
            mir,
            sigs,
            &dependency.indices.functions,
        )
        .map_err(|e| e.to_string())?;
        summary.finish().map_err(|e| e.to_string())?;
        Some(facts)
    };
    Ok(LoadedIr {
        required_instances,
        function_summary,
    })
}

struct DependencyWork<'a> {
    index: usize,
    weight: usize,
    hir: PreparedHir<'a>,
    signature_bytes: Option<Result<&'a [u8], String>>,
}

impl<'a> DependencyWork<'a> {
    fn decode(self) -> Result<DecodedDependency<'a>, String> {
        self.hir.decode().map(|hir| {
            let signatures = self.signature_bytes.map(|bytes| {
                let mut types = Decoder::with_ids(bytes?, hir.names.ids.clone());
                DecodedSignatureRows::decode(&mut types).map_err(|error| error.to_string())
            });
            DecodedDependency { hir, signatures }
        })
    }
}

struct DecodedDependency<'a> {
    hir: DecodedHir<'a>,
    signatures: Option<Result<DecodedSignatureRows, String>>,
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
    decode_manifest(artifact.section(1)?, artifact.has_object())
}

fn decode_manifest(bytes: &[u8], linkable: bool) -> Result<Manifest, String> {
    let mut d = Decoder::new(bytes);
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
    bytes: std::sync::Arc<Vec<u8>>,
    index: std::sync::Arc<ArtifactIndex>,
    pub manifest: Manifest,
}

impl Resolved {
    fn new(path: std::path::PathBuf, bytes: Vec<u8>) -> Result<Self, String> {
        let index = std::sync::Arc::new(ArtifactIndex::decode(&bytes)?);
        let manifest = manifest_from_view(index.view(&bytes))?;
        Ok(Self {
            path,
            bytes: std::sync::Arc::new(bytes),
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
    let hashes: Vec<_> = known.iter().map(|dependency| dependency.hash).collect();
    resolve_paths_with_roots(paths, &hashes, None, None, ArchiveRead::Full)
        .map(|(artifacts, _)| artifacts)
}

pub(crate) fn resolve_paths_with_roots(
    paths: &[std::path::PathBuf],
    known: &[[u8; 32]],
    dependency_files: Option<&[&std::path::Path]>,
    cache: Option<&FreshArtifacts>,
    read: ArchiveRead,
) -> Result<(Vec<Resolved>, Vec<[u8; 32]>), String> {
    let mut prefetched = match (cache, dependency_files) {
        (Some(cache), Some(files)) => cache.prefetch(paths, files, read),
        _ => Default::default(),
    };
    resolve_with(
        paths,
        known,
        dependency_files,
        |path| {
            if let Some(result) = prefetched.remove(path) {
                return result;
            }
            if let Some(cache) = cache {
                return cache.read_resolved(path, read);
            }
            let bytes =
                std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
            Resolved::new(path.to_owned(), bytes).map_err(|e| format!("{}: {e}", path.display()))
        },
        |artifact| &artifact.manifest,
    )
}

fn resolve_with<T>(
    paths: &[std::path::PathBuf],
    known: &[[u8; 32]],
    dependency_files: Option<&[&std::path::Path]>,
    mut load: impl FnMut(&std::path::Path) -> Result<T, String>,
    manifest: impl Copy + Fn(&T) -> &Manifest,
) -> Result<(Vec<T>, Vec<[u8; 32]>), String> {
    let mut candidates = std::collections::BTreeMap::new();
    let mut explicit = std::collections::BTreeMap::new();
    let mut roots = Vec::new();
    let mut directories = std::collections::BTreeSet::new();
    for path in paths {
        if let Some(&hash) = explicit.get(path) {
            roots.push(hash);
            continue;
        }
        let artifact = load(path)?;
        let hash = manifest(&artifact).hash;
        roots.push(hash);
        explicit.insert(path.clone(), hash);
        candidates.insert(hash, artifact);
        if dependency_files.is_none() {
            directories.insert(
                path.parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .unwrap_or_else(|| std::path::Path::new("."))
                    .to_owned(),
            );
        }
    }
    for &path in dependency_files.into_iter().flatten() {
        if explicit.contains_key(path) {
            continue;
        }
        let artifact = load(path)?;
        candidates
            .entry(manifest(&artifact).hash)
            .or_insert(artifact);
    }
    let need_search = dependency_files.is_none()
        && candidates.values().any(|artifact| {
            manifest(artifact)
                .dependencies
                .iter()
                .any(|hash| !known.contains(hash) && !candidates.contains_key(hash))
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
            let Ok(artifact) = load(&path) else {
                continue;
            };
            candidates
                .entry(manifest(&artifact).hash)
                .or_insert(artifact);
        }
    }
    fn visit<T>(
        hash: [u8; 32],
        candidates: &mut std::collections::BTreeMap<[u8; 32], T>,
        known: &mut std::collections::BTreeSet<[u8; 32]>,
        active: &mut std::collections::BTreeSet<[u8; 32]>,
        out: &mut Vec<T>,
        manifest: impl Copy + Fn(&T) -> &Manifest,
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
        for &dependency in &manifest(&artifact).dependencies {
            visit(dependency, candidates, known, active, out, manifest)?;
        }
        active.remove(&hash);
        known.insert(hash);
        out.push(artifact);
        Ok(())
    }
    let mut known = known.iter().copied().collect();
    let mut active = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for &root in &roots {
        visit(
            root,
            &mut candidates,
            &mut known,
            &mut active,
            &mut out,
            manifest,
        )?;
    }
    Ok((out, roots))
}

#[cfg(test)]
#[path = "../../../../tests/support/scratch.rs"]
mod scratch;

#[cfg(test)]
mod dependency_tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn package_dependency_files_keep_direct_roots_and_skip_discovery() {
        let root = PathBuf::from("unavailable/root.flib");
        let dependency = PathBuf::from("unavailable/dependency.flib");
        let paths = [root.clone()];
        let files = [root.as_path(), dependency.as_path()];
        let load = |path: &Path| {
            let (hash, dependencies) = if path == root {
                ([1; 32], vec![[2; 32]])
            } else {
                assert_eq!(path, dependency);
                ([2; 32], Vec::new())
            };
            Ok(Manifest {
                name: path.display().to_string(),
                hash,
                dependencies,
                linkable: true,
            })
        };

        let (artifacts, roots) = resolve_with(&paths, &[], Some(&files), load, |m| m).unwrap();
        assert_eq!(roots, [[1; 32]]);
        assert_eq!(
            artifacts.iter().map(|m| m.hash).collect::<Vec<_>>(),
            [[2; 32], [1; 32]]
        );
        let incomplete = resolve_with(&paths, &[], Some(&files[..1]), load, |m| m).unwrap_err();
        assert!(
            incomplete.starts_with("missing fluxlib dependency"),
            "{incomplete}"
        );
    }

    #[test]
    fn parallel_archive_reads_report_the_first_requested_error() {
        let scratch = super::scratch::Scratch::new(std::env::temp_dir().join("flux-read-tests"));
        let paths: Vec<_> = (0..8)
            .rev()
            .map(|index| scratch.path().join(format!("{index}.flib")))
            .collect();
        std::fs::write(&paths[0], b"invalid archive").unwrap();
        let files: Vec<_> = paths.iter().map(PathBuf::as_path).collect();
        let cache = FreshArtifacts::for_outputs(scratch.path());
        let resolve = || {
            resolve_paths_with_roots(
                &paths[..1],
                &[],
                Some(&files),
                Some(&cache),
                ArchiveRead::Full,
            )
            .unwrap_err()
        };
        let serial = resolve();
        assert!(serial.contains("invalid fluxlib archive"), "{serial}");
        assert_eq!(crate::backend_workers::with_limit(4, resolve), serial);
    }

    #[cfg(unix)]
    #[test]
    fn verified_archive_index_is_reused_and_invalidated_on_write() {
        let scratch = super::scratch::Scratch::new(std::env::temp_dir().join("flux-index-tests"));
        let path = scratch.path().join("dep.flib");
        let mut identity = Encoder::new();
        "dep".encode(&mut identity).unwrap();
        vec![0u8; 32].encode(&mut identity).unwrap();
        Vec::<Vec<u8>>::new().encode(&mut identity).unwrap();
        let object = vec![17, 42, 50];
        let mut linkage = Encoder::new();
        (
            2u32,
            artifact_target().unwrap(),
            Some(super::super::digest(&object).to_vec()),
        )
            .encode(&mut linkage)
            .unwrap();
        linkage.blob(&[]).unwrap();
        let sections = (1..=9)
            .map(|tag| Section {
                tag,
                bytes: match tag {
                    1 => identity_bytes(&mut identity),
                    9 => identity_bytes(&mut linkage),
                    _ => Vec::new(),
                },
            })
            .collect();
        let mut archive = Archive {
            metadata: Metadata {
                compiler: compiler_identity().unwrap(),
                sections,
            }
            .encode()
            .unwrap(),
            object: Some(object),
        };
        std::fs::write(&path, archive.encode().unwrap()).unwrap();
        std::thread::sleep(std::time::Duration::from_secs(2));
        let cache = FreshArtifacts::for_outputs(scratch.path());
        let first = cache.read_resolved(&path, ArchiveRead::Full).unwrap();
        let second = cache.read_resolved(&path, ArchiveRead::Full).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first.index, &second.index));

        let partial = cache.read_resolved(&path, ArchiveRead::Metadata).unwrap();
        assert!(partial.manifest.linkable);
        assert!(partial.index.view(partial.bytes()).object().is_none());
        assert_eq!(partial.bytes(), archive.metadata);

        // A metadata-only read must never satisfy a later full-object read.
        archive.object.as_mut().unwrap()[0] ^= 1;
        std::fs::write(&path, archive.encode().unwrap()).unwrap();
        assert!(cache.read_resolved(&path, ArchiveRead::Metadata).is_ok());
        let error = cache.read_resolved(&path, ArchiveRead::Full).unwrap_err();
        assert!(error.contains("object checksum"), "{error}");

        let mut bad_padding = archive.encode().unwrap();
        *bad_padding.last_mut().unwrap() = 0;
        std::fs::write(&path, bad_padding).unwrap();
        let error = cache
            .read_resolved(&path, ArchiveRead::Metadata)
            .unwrap_err();
        assert!(error.contains("archive padding"), "{error}");

        archive.metadata[0] ^= 1;
        std::fs::write(&path, archive.encode().unwrap()).unwrap();
        assert!(cache.read_resolved(&path, ArchiveRead::Full).is_err());
        assert!(cache.read_resolved(&path, ArchiveRead::Metadata).is_err());
    }
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
    pub function_summaries: flux_mir::imported::ImportedFunctionSummaries,
    /// Exact identities in the order of the explicit paths passed to `load`.
    pub roots: Vec<[u8; 32]>,
}

fn embedded_hash(compiler: &[u8; 32], name: &str) -> [u8; 32] {
    let mut bytes = compiler.to_vec();
    bytes.extend_from_slice(name.as_bytes());
    crate::flib::digest(&bytes)
}

/// Freshness uses crate identities without constructing a semantic state.
pub(crate) fn embedded_hashes() -> Result<Vec<[u8; 32]>, String> {
    let compiler = compiler_identity()?;
    if crate::builtins::has_staged_libraries() {
        return Ok(flux_hir::flib::BUILTIN_CRATE_NAMES
            .into_iter()
            .map(|name| embedded_hash(&compiler, name))
            .collect());
    }
    Ok(flux_hir::Core::get()
        .items
        .crates
        .iter()
        .map(|krate| embedded_hash(&compiler, krate.name.as_str()))
        .collect())
}

/// Immutable, integrity-checked inputs retained across HIR and semantic loading.
pub(crate) struct DependencyInputs {
    artifacts: Vec<Resolved>,
    roots: Vec<[u8; 32]>,
}

impl DependencyInputs {
    pub(crate) fn resolve(
        paths: &[std::path::PathBuf],
        known: &[[u8; 32]],
        dependency_files: Option<&[&std::path::Path]>,
        cache: Option<&FreshArtifacts>,
        read: ArchiveRead,
    ) -> Result<Self, String> {
        super::time("dependency discovery", || {
            let (artifacts, roots) =
                resolve_paths_with_roots(paths, known, dependency_files, cache, read)?;
            Ok(Self { artifacts, roots })
        })
    }

    /// Each validated metadata checksum binds all sections, including the
    /// expected native-object digest. Order also binds the loading context.
    pub(crate) fn checksums(&self) -> Vec<u8> {
        if self.artifacts.is_empty() {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(33 + self.artifacts.len() * 32);
        if let Some(checksum) = crate::builtins::payload_checksum() {
            result.push(1);
            result.extend_from_slice(&checksum);
        } else {
            result.push(0);
        }
        for artifact in &self.artifacts {
            let metadata = artifact.index.view(&artifact.bytes).metadata_bytes();
            result.extend_from_slice(&metadata[metadata.len() - 32..]);
        }
        result
    }

    pub(crate) fn all_linkable(&self, staging: Option<&std::path::Path>) -> bool {
        self.artifacts.iter().all(|artifact| {
            artifact.manifest.linkable
                || staging.is_some_and(|staging| artifact.path.parent() == Some(staging))
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Recheck {
    Full,
    LowerBodies,
    Collect,
}

/// Reads the prior checked output only from the package build's own cache.
pub(crate) fn reusable_library(
    output: &std::path::Path,
    checksums: &[u8],
    cache: Option<&FreshArtifacts>,
) -> Result<(Manifest, Recheck), String> {
    let artifact = if let Some(cache) = cache {
        cache.read_resolved(output, ArchiveRead::Full)?
    } else {
        let bytes = std::fs::read(output).map_err(|e| e.to_string())?;
        Resolved::new(output.to_owned(), bytes)?
    };
    let view = artifact.index.view(&artifact.bytes);
    let recheck = match view.section(4)?.split_last() {
        Some((&flag, recorded)) if !checksums.is_empty() && recorded == checksums => match flag {
            0 => Recheck::LowerBodies,
            1 => Recheck::Collect,
            _ => Recheck::Full,
        },
        _ => Recheck::Full,
    };
    Ok((artifact.manifest, recheck))
}

impl DependencyState {
    /// Starts with the compiler-owned embedded libraries and their signature tables.
    pub fn new() -> Result<Self, String> {
        if let Some(suite) = crate::builtins::active() {
            Self::from_interface(&suite.db, &suite.items, &suite.sigs)
        } else {
            Self::with_prefix(
                SourceDb::new(),
                flux_hir::Core::get().items.clone(),
                flux_ty::LibraryTypes::get().sigs.fork_for_extension(),
            )
        }
    }

    pub(crate) fn from_interface(
        db: &SourceDb,
        items: &ItemTree,
        sigs: &Signatures,
    ) -> Result<Self, String> {
        Self::with_prefix(db.fork(), items.clone(), sigs.fork_for_extension())
    }

    fn with_prefix(db: SourceDb, mut items: ItemTree, sigs: Signatures) -> Result<Self, String> {
        items.core = items.counts();
        let compiler = compiler_identity()?;
        let dependencies = items
            .crates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                let krate = CrateId(i as u32);
                Dependency {
                    krate,
                    hash: embedded_hash(&compiler, c.name.as_str()),
                    indices: CrateIndices::collect(&items, krate),
                }
            })
            .collect();
        Ok(Self {
            db,
            items,
            sigs,
            mir: FileMir {
                panic_strategy: crate::builtins::selected_strategy(),
                deferred_functions: Default::default(),
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
            function_summaries: Default::default(),
            roots: Vec::new(),
        })
    }

    /// Loads exact artifact dependencies. Failure consumes the unpublished staging state.
    pub fn load(self, paths: &[std::path::PathBuf]) -> Result<Self, String> {
        self.load_with_files(paths, None)
    }

    pub(crate) fn load_with_files(
        self,
        paths: &[std::path::PathBuf],
        dependency_files: Option<&[&std::path::Path]>,
    ) -> Result<Self, String> {
        let inputs = self.resolve(paths, dependency_files, None, ArchiveRead::Full)?;
        self.load_inputs(&inputs, false, false, true)
    }

    pub(crate) fn resolve(
        &self,
        paths: &[std::path::PathBuf],
        dependency_files: Option<&[&std::path::Path]>,
        cache: Option<&FreshArtifacts>,
        read: ArchiveRead,
    ) -> Result<DependencyInputs, String> {
        let known: Vec<_> = self.dependencies.iter().map(|dep| dep.hash).collect();
        DependencyInputs::resolve(paths, &known, dependency_files, cache, read)
    }

    fn prepare_dependency_batch<'a>(
        &mut self,
        artifacts: &'a [Resolved],
        resolver: &std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
        decode_signatures: bool,
    ) -> (Vec<DependencyWork<'a>>, Option<String>) {
        let mut layout = ItemArchiveLayout::new(&self.items);
        let mut prepared: Vec<DependencyWork<'a>> = Vec::new();
        let mut first_error = None;
        for (index, artifact) in artifacts.iter().enumerate() {
            let view = artifact.index.view(&artifact.bytes);
            let result = prepare_hir(
                view,
                |hash| {
                    self.dependencies
                        .iter()
                        .find(|dep| &dep.hash == hash)
                        .map(|dep| (dep.krate, &dep.indices))
                        .or_else(|| {
                            prepared
                                .iter()
                                .find(|work| &work.hir.hash == hash)
                                .map(|work| work.hir.prepared.identity())
                        })
                },
                resolver.clone(),
                &mut layout,
                &mut self.db,
            );
            match result {
                Ok(row) => {
                    let signature_bytes = decode_signatures.then(|| view.section(5));
                    let weight = row.names.remaining()
                        + signature_bytes
                            .as_ref()
                            .and_then(|bytes| bytes.as_ref().ok())
                            .map_or(0, |bytes| bytes.len());
                    prepared.push(DependencyWork {
                        index,
                        weight,
                        hir: row,
                        signature_bytes,
                    });
                }
                Err(error) => {
                    first_error = Some(error);
                    break;
                }
            }
        }
        (prepared, first_error)
    }

    pub(crate) fn load_inputs(
        mut self,
        inputs: &DependencyInputs,
        hir_only: bool,
        defer_functions: bool,
        link_dependency_objects: bool,
    ) -> Result<Self, String> {
        let resolver: std::sync::Arc<dyn flux_hir::ProcMacroResolver> =
            std::sync::Arc::new(crate::proc_macro::FlibHostResolver::default());
        self.roots.clone_from(&inputs.roots);
        let total = crate::backend_workers::limit().get();
        let workers = total.min(inputs.artifacts.len() / 4);
        let flags = (hir_only, defer_functions, link_dependency_objects);
        if workers <= 1 {
            for artifact in &inputs.artifacts {
                self.append_dependency(artifact, None, &resolver, flags)?;
            }
            return Ok(self);
        }

        use std::sync::atomic::{AtomicUsize, Ordering};
        let (mut work, first_error) = super::time("prepare dependency layouts", || {
            self.prepare_dependency_batch(&inputs.artifacts, &resolver, !hir_only)
        });
        let mut decoded = (0..inputs.artifacts.len())
            .map(|_| None)
            .collect::<Vec<_>>();
        if let Some(error) = first_error {
            decoded[work.len()] = Some(Err(error));
        }
        // Small batches finish quickly with all slots decoding. With a
        // larger batch, leave a slot for ordered append and prioritize its
        // first inputs while giving larger archives a head start.
        let streaming = workers >= 4;
        let helpers = if streaming {
            workers.min(total - 1)
        } else {
            workers - 1
        };
        work.sort_unstable_by_key(|row| {
            (
                streaming && row.index >= helpers * 2,
                std::cmp::Reverse(row.weight),
            )
        });
        let pending = std::sync::Mutex::new(work.into_iter());
        let running = AtomicUsize::new(0);
        let (sender, receiver) = std::sync::mpsc::channel();
        let drain = |sender: std::sync::mpsc::Sender<_>| {
            loop {
                let next = pending
                    .lock()
                    .expect("dependency HIR queue poisoned")
                    .next();
                let Some(work) = next else { break };
                let index = work.index;
                if sender.send((index, work.decode())).is_err() {
                    break;
                }
            }
            running.fetch_sub(1, Ordering::Release);
        };
        std::thread::scope(|scope| {
            for _ in 0..helpers {
                let tx = sender.clone();
                running.fetch_add(1, Ordering::Relaxed);
                if std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024)
                    .spawn_scoped(scope, move || drain(tx))
                    .is_err()
                {
                    running.fetch_sub(1, Ordering::Relaxed);
                    break;
                }
            }
            if !streaming || running.load(Ordering::Acquire) == 0 {
                running.fetch_add(1, Ordering::Relaxed);
                drain(sender);
            } else {
                drop(sender);
            }
            if !streaming {
                for (index, result) in &receiver {
                    decoded[index] = Some(result);
                }
            }
            for (index, artifact) in inputs.artifacts.iter().enumerate() {
                while decoded[index].is_none() {
                    let (ready, result): (usize, _) =
                        receiver.recv().expect("dependency decoder stopped");
                    decoded[ready] = Some(result);
                }
                let result = decoded[index].take().expect("dependency decoded");
                let result =
                    result.map_err(|error| format!("{}: {error}", artifact.path.display()))?;
                // A decoder that has exited can lend its slot to the append
                // stage, which also parallelizes large MIR sections.
                let idle = total - running.load(Ordering::Acquire);
                crate::backend_workers::with_limit(idle, || {
                    self.append_dependency(artifact, Some(result), &resolver, flags)
                })?;
            }
            Ok::<_, String>(())
        })?;
        Ok(self)
    }

    fn append_dependency(
        &mut self,
        artifact: &Resolved,
        decoded: Option<DecodedDependency<'_>>,
        resolver: &std::sync::Arc<dyn flux_hir::ProcMacroResolver>,
        (hir_only, defer_functions, link_dependency_objects): (bool, bool, bool),
    ) -> Result<(), String> {
        let context = |error| format!("{}: {error}", artifact.path.display());
        let view = artifact.index.view(&artifact.bytes);
        let ((dependency, ids), signatures) = if let Some(decoded) = decoded {
            let hir = super::time("HIR append", || {
                decoded.hir.append(&mut self.items, &mut self.db)
            })
            .map_err(context)?;
            (hir, decoded.signatures)
        } else {
            let hir = load_hir(
                view,
                &mut self.db,
                &mut self.items,
                &self.dependencies,
                resolver.clone(),
            )
            .map_err(context)?;
            (hir, None)
        };
        if dependency.hash != artifact.manifest.hash {
            return Err(context("inconsistent artifact identity".into()));
        }
        if !hir_only {
            let ir = load_ir(
                view,
                &self.items,
                &mut self.sigs,
                &mut self.mir,
                &dependency,
                ids,
                (
                    SignatureInput::Append(signatures),
                    defer_functions.then_some(artifact),
                ),
            )
            .map_err(context)?;
            self.required_instances.extend(ir.required_instances);
            self.function_summaries
                .append(ir.function_summary, &dependency.indices.functions);
            if link_dependency_objects && view.has_object() {
                let object = view
                    .object()
                    .ok_or_else(|| context("dependency object was not read for linking".into()))?;
                self.objects.push((artifact.path.clone(), object.to_vec()));
            }
        }
        self.dependencies.push(dependency);
        Ok(())
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

type OutputLock = std::sync::Arc<std::sync::Mutex<()>>;

/// Validated metadata shared by the workers of one package build. The cache
/// retains identities and freshness snapshots, and drops the archive bytes.
#[derive(Default)]
pub(crate) struct FreshArtifacts {
    entries: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, StampedArtifact>>,
    indexes: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, StampedIndex>>,
    output_directory: Option<std::path::PathBuf>,
    output_locks: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, OutputLock>>,
}

struct StampedArtifact {
    stamp: super::executable::Stamp,
    artifact: std::sync::Arc<FreshArtifact>,
}

struct StampedIndex {
    stamp: super::executable::Stamp,
    index: std::sync::Arc<ArtifactIndex>,
    manifest: Manifest,
}

pub(crate) struct FreshArtifact {
    manifest: Manifest,
    snapshot: Option<super::freshness::Snapshot>,
}

pub(crate) struct DependencyIdentities {
    pub ordered: Vec<[u8; 32]>,
    pub explicit: Vec<[u8; 32]>,
}

impl FreshArtifacts {
    pub(crate) fn output_lock(&self, path: &std::path::Path) -> OutputLock {
        self.output_locks
            .lock()
            .expect("shared host output locks poisoned")
            .entry(path.to_owned())
            .or_default()
            .clone()
    }

    /// Only a package build's own output directory gets disk cache records.
    pub(crate) fn for_outputs(directory: &std::path::Path) -> Self {
        Self {
            output_directory: Some(directory.to_owned()),
            ..Self::default()
        }
    }

    /// Reads explicitly enumerated package inputs within the scheduler's worker
    /// budget. The resolver consumes each result in its original input order.
    fn prefetch<'a>(
        &self,
        paths: &'a [std::path::PathBuf],
        files: &[&'a std::path::Path],
        read: ArchiveRead,
    ) -> std::collections::BTreeMap<&'a std::path::Path, Result<Resolved, String>> {
        use std::sync::atomic::{AtomicUsize, Ordering};

        const MIN_FILES_PER_WORKER: usize = 4;
        let budget = crate::backend_workers::limit().get();
        if budget < 2 || paths.len() + files.len() < 2 * MIN_FILES_PER_WORKER {
            return Default::default();
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut queue: Vec<_> = paths
            .iter()
            .map(std::path::PathBuf::as_path)
            .chain(files.iter().copied())
            .filter(|path| seen.insert(*path))
            .collect();
        let workers = budget.min(queue.len() / MIN_FILES_PER_WORKER);
        if workers < 2 {
            return Default::default();
        }
        // Larger archives take longer to read and verify. Start them first
        // so one large archive does not outlast the other workers at the end.
        queue.sort_by_cached_key(|path| {
            std::cmp::Reverse(std::fs::metadata(path).map_or(0, |metadata| metadata.len()))
        });

        let next = AtomicUsize::new(0);
        let drain = || {
            std::iter::from_fn(|| queue.get(next.fetch_add(1, Ordering::Relaxed)).copied())
                .map(|path| (path, self.read_resolved(path, read)))
                .collect::<Vec<_>>()
        };
        std::thread::scope(|scope| {
            let mut helpers = Vec::new();
            for _ in 1..workers {
                let Ok(helper) = std::thread::Builder::new().spawn_scoped(scope, drain) else {
                    break;
                };
                helpers.push(helper);
            }
            let mut results: std::collections::BTreeMap<_, _> = drain().into_iter().collect();
            for helper in helpers {
                results.extend(helper.join().expect("dependency artifact reader panicked"));
            }
            results
        })
    }

    /// Reads a new byte buffer, reusing a verified index only while the
    /// settled file stamp is unchanged. Neither the archive bytes nor its
    /// decoded HIR are retained by the package cache.
    fn read_resolved(&self, path: &std::path::Path, read: ArchiveRead) -> Result<Resolved, String> {
        use super::executable::Stamp;
        use std::io::Read;

        let io_error = |error| format!("cannot read {}: {error}", path.display());
        let mut file = std::fs::File::open(path).map_err(io_error)?;
        let metadata = file.metadata().map_err(io_error)?;
        let length = metadata.len();
        let before = Stamp::of(&metadata);
        let managed = self
            .output_directory
            .as_ref()
            .is_some_and(|root| path.starts_with(root));
        if read == ArchiveRead::Metadata && managed {
            let (archive, bytes) = ArchiveIndex::read_metadata(&mut file, length)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            if Stamp::of(&file.metadata().map_err(io_error)?) != before {
                return Err(format!("{} changed while reading", path.display()));
            }
            let index = ArtifactIndex::from_framing(archive, &bytes, read)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let manifest = manifest_from_view(index.view(&bytes))?;
            return Ok(Resolved {
                path: path.to_owned(),
                bytes: std::sync::Arc::new(bytes),
                index: std::sync::Arc::new(index),
                manifest,
            });
        }
        let cacheable = before.settled() && managed;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(io_error)?;
        if Stamp::of(&file.metadata().map_err(io_error)?) != before {
            return Err(format!("{} changed while reading", path.display()));
        }
        if !cacheable {
            return Resolved::new(path.to_owned(), bytes)
                .map_err(|error| format!("{}: {error}", path.display()));
        }

        let indexes = self
            .indexes
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        if let Some(previous) = indexes.get(path).filter(|entry| entry.stamp == before) {
            return Ok(Resolved {
                path: path.to_owned(),
                bytes: std::sync::Arc::new(bytes),
                index: previous.index.clone(),
                manifest: previous.manifest.clone(),
            });
        }
        // Independent workers can validate different artifacts in parallel.
        drop(indexes);
        let artifact = Resolved::new(path.to_owned(), bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        self.remember_index(
            path,
            before,
            artifact.index.clone(),
            artifact.manifest.clone(),
        );
        Ok(artifact)
    }

    fn remember_index(
        &self,
        path: &std::path::Path,
        stamp: super::executable::Stamp,
        index: std::sync::Arc<ArtifactIndex>,
        manifest: Manifest,
    ) {
        self.indexes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(
                path.to_owned(),
                StampedIndex {
                    stamp,
                    index,
                    manifest,
                },
            );
    }

    pub(crate) fn read(
        &self,
        path: &std::path::Path,
    ) -> Result<std::sync::Arc<FreshArtifact>, String> {
        use super::executable::Stamp;
        use std::io::Read;

        let read_error = |error| format!("cannot read {}: {error}", path.display());
        let mut file = std::fs::File::open(path).map_err(read_error)?;
        let before = Stamp::of(&file.metadata().map_err(read_error)?);
        let mut entries = before.settled().then(|| {
            self.entries
                .lock()
                .unwrap_or_else(|error| error.into_inner())
        });
        if let Some(cached) = entries.as_ref().and_then(|entries| entries.get(path))
            && cached.stamp == before
        {
            return Ok(cached.artifact.clone());
        }
        let record = self
            .output_directory
            .as_ref()
            .filter(|root| entries.is_some() && path.starts_with(root))
            .map(|_| fresh_record::path(path));
        let cached = record.as_ref().and_then(|record| {
            super::time("freshness record lookup", || {
                fresh_record::read(record, before)
            })
        });
        let artifact = if let Some(cached) = cached {
            if before != Stamp::of(&file.metadata().map_err(read_error)?) {
                return Err(format!("{} changed while reading", path.display()));
            }
            cached
        } else {
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).map_err(read_error)?;
            let index = ArtifactIndex::decode(&bytes)
                .map_err(|error| format!("{}: {error}", path.display()))?;
            let view = index.view(&bytes);
            let manifest = manifest_from_view(view)?;
            let snapshot_bytes = view.section(2)?;
            let snapshot = super::freshness::Snapshot::from_bytes(snapshot_bytes).ok();
            let after = Stamp::of(&file.metadata().map_err(read_error)?);
            if before != after {
                return Err(format!("{} changed while reading", path.display()));
            }
            if let Some(record) = record {
                fresh_record::record(
                    &record,
                    before,
                    view.section(1)?,
                    snapshot_bytes,
                    manifest.linkable,
                );
                self.remember_index(path, before, std::sync::Arc::new(index), manifest.clone());
            }
            FreshArtifact { manifest, snapshot }
        };
        let artifact = std::sync::Arc::new(artifact);
        if let Some(entries) = &mut entries {
            entries.insert(
                path.to_owned(),
                StampedArtifact {
                    stamp: before,
                    artifact: artifact.clone(),
                },
            );
        }
        Ok(artifact)
    }

    pub(crate) fn dependencies(
        &self,
        paths: &[std::path::PathBuf],
        known: &[[u8; 32]],
        dependency_files: Option<&[&std::path::Path]>,
    ) -> Result<DependencyIdentities, String> {
        let (artifacts, roots) = resolve_with(
            paths,
            known,
            dependency_files,
            |path| self.read(path),
            |artifact| &artifact.manifest,
        )?;
        Ok(DependencyIdentities {
            ordered: artifacts
                .iter()
                .map(|artifact| artifact.manifest.hash)
                .collect(),
            explicit: roots,
        })
    }
}

impl FreshArtifact {
    pub(crate) fn is_fresh(
        &self,
        configuration: &[u8],
        dependencies: &[[u8; 32]],
        environment: Option<&std::collections::BTreeMap<String, String>>,
        capability: Capability,
    ) -> Result<bool, String> {
        if capability == Capability::Object && !self.manifest.linkable {
            return Ok(false);
        }
        self.snapshot.as_ref().map_or(Ok(false), |snapshot| {
            snapshot.is_fresh(configuration, dependencies, environment)
        })
    }
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
    match (checksum, artifact.has_object()) {
        (None, false) => Ok(false),
        (Some(checksum), true)
            if checksum.len() == 32
                && artifact
                    .object()
                    .is_none_or(|bytes| checksum.as_slice() == crate::flib::digest(bytes)) =>
        {
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
