//! Independently decodable groups of imported function records.

use super::{decode_function_body, decode_map_key};
use crate::{Body, FileMir};
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, IdMap};
use flux_hir::{Function, FunctionId, ItemTree};
use flux_ty::{Signatures, Ty};
use rustc_hash::FxHashSet;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

pub(super) const HEADER: u32 = u32::MAX - 1;
const GROUP_SIZE: usize = 64;
const MIN_ROWS_PER_WORKER: usize = 512;
// Managed consumers benefit from skipping groups even when eager decoding
// would be too small to split across workers.
pub(crate) const MIN_INDEXED_FUNCTIONS: usize = GROUP_SIZE;

pub(super) fn encode(bodies: &[(FunctionId, &Body)], e: &mut Encoder) -> Result<(), Error> {
    for group in bodies.chunks(GROUP_SIZE) {
        group[0].0.encode(e)?;
        e.independent_records(|record| {
            for &(id, body) in group {
                id.encode(record)?;
                body.encode(record)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

struct Chunk<'a> {
    first: FunctionId,
    count: usize,
    bytes: &'a [u8],
}

type FunctionRows = Vec<(FunctionId, Body)>;

/// The checked bytes and item order of a package-managed dependency.
pub struct DeferredFunctionSource<'a> {
    pub bytes: Arc<Vec<u8>>,
    pub section_end: usize,
    pub functions: &'a [u32],
}

#[derive(Default, Debug)]
pub struct DeferredFunctions {
    archives: Vec<DeferredArchive>,
    locations: rustc_hash::FxHashMap<FunctionId, (usize, usize)>,
    pub(crate) builtin_prefix: Option<Arc<FileMir>>,
}

#[derive(Debug)]
struct DeferredArchive {
    bytes: Arc<Vec<u8>>,
    ids: IdMap,
    budget: flux_hir::flib::AllocationBudget,
    functions: Vec<FunctionId>,
    chunks: Vec<DeferredChunk>,
}

#[derive(Debug)]
struct DeferredChunk {
    payload: Range<usize>,
    result: OnceLock<Result<FunctionRows, Error>>,
}

impl DeferredFunctions {
    /// The number of decoded and indexed chunks in this compilation.
    pub fn chunk_counts(&self) -> (usize, usize) {
        self.archives
            .iter()
            .flat_map(|archive| &archive.chunks)
            .fold((0, 0), |(decoded, total), chunk| {
                (
                    decoded + usize::from(chunk.result.get().is_some()),
                    total + 1,
                )
            })
    }

    pub(crate) fn contains(&self, id: FunctionId) -> bool {
        self.locations.contains_key(&id)
    }

    pub(crate) fn error(&self) -> Option<&Error> {
        self.archives
            .iter()
            .flat_map(|archive| &archive.chunks)
            .find_map(|chunk| chunk.result.get()?.as_ref().err())
    }

    pub(crate) fn cached(&self, id: FunctionId) -> Option<&Body> {
        let &(archive, ordinal) = self.locations.get(&id)?;
        let chunk = &self.archives[archive].chunks[ordinal / GROUP_SIZE];
        let rows = chunk.result.get()?.as_ref().ok()?;
        Some(&rows[ordinal % GROUP_SIZE].1)
    }

    pub(crate) fn get<'a>(
        &'a self,
        mir: &FileMir,
        items: &ItemTree,
        sigs: &Signatures,
        id: FunctionId,
    ) -> Result<Option<&'a Body>, Error> {
        let Some(&(archive, ordinal)) = self.locations.get(&id) else {
            return Ok(None);
        };
        let archive = &self.archives[archive];
        let group = ordinal / GROUP_SIZE;
        let chunk = &archive.chunks[group];
        let start = group * GROUP_SIZE;
        let expected = &archive.functions[start..(start + GROUP_SIZE).min(archive.functions.len())];
        let rows = chunk
            .result
            .get_or_init(|| {
                let mut ids = archive.ids.clone();
                let mut rows = Vec::with_capacity(expected.len());
                let cx = Context {
                    items,
                    sigs,
                    budget: archive.budget.clone(),
                    check_types: true,
                };
                let _fields = flux_solve::remember_field_types(sigs);
                let _queries = flux_solve::remember_solver_queries(items, sigs);
                let mut closure_types = Vec::new();
                cx.decode(
                    &[Chunk {
                        first: expected[0],
                        count: expected.len(),
                        bytes: &archive.bytes[chunk.payload.clone()],
                    }],
                    &mut ids,
                    &mut FxHashSet::default(),
                    |id, body| {
                        if id != expected[rows.len()] {
                            return Err(Error::new("MIR chunk does not match its function index"));
                        }
                        if mir.panic_strategy == crate::PanicStrategy::Abort && body.has_unwind() {
                            return Err(Error::new("exceptional MIR requires panic=unwind"));
                        }
                        closure_types.extend(body.locals.values().map(|local| local.ty));
                        rows.push((id, body));
                        Ok(())
                    },
                )?;
                mir.validate_imported_closure_types(&ids, closure_types)?;
                Ok(rows)
            })
            .as_ref()
            .map_err(Clone::clone)?;
        Ok(Some(&rows[ordinal % GROUP_SIZE].1))
    }
}

struct Context<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    budget: flux_hir::flib::AllocationBudget,
    check_types: bool,
}

impl Context<'_> {
    fn decode(
        &self,
        chunks: &[Chunk<'_>],
        ids: &mut IdMap,
        validated_types: &mut FxHashSet<(Ty, u32)>,
        mut emit: impl FnMut(FunctionId, Body) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let mut previous = None;
        for chunk in chunks {
            let mut d = Decoder::with_ids(chunk.bytes, std::mem::take(ids))
                .with_allocation_budget(self.budget.clone());
            for index in 0..chunk.count {
                let id = decode_map_key::<Function>(&mut d)?;
                if (index == 0 && id != chunk.first)
                    || previous.is_some_and(|previous| previous >= id)
                {
                    return Err(Error::new("invalid MIR chunk function order"));
                }
                let body = decode_function_body(
                    &mut d,
                    self.items,
                    self.sigs,
                    id,
                    self.check_types,
                    validated_types,
                )?;
                emit(id, body)?;
                previous = Some(id);
            }
            d.finish()?;
            *ids = d.ids;
        }
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_into(
    mir: &mut FileMir,
    d: &mut Decoder<'_>,
    items: &ItemTree,
    sigs: &Signatures,
    count: usize,
    workers: usize,
    validated_types: &mut FxHashSet<(Ty, u32)>,
    source: Option<&DeferredFunctionSource<'_>>,
) -> Result<(), Error> {
    let mut chunks: Vec<Chunk<'_>> = Vec::with_capacity(count.div_ceil(GROUP_SIZE));
    let mut spans = Vec::new();
    for start in (0..count).step_by(GROUP_SIZE) {
        let first = decode_map_key::<Function>(d)?;
        if chunks.last().is_some_and(|chunk| chunk.first >= first) {
            return Err(Error::new("invalid MIR chunk index order"));
        }
        let size = u32::decode(d)? as usize;
        if let Some(source) = source {
            let start = source
                .section_end
                .checked_sub(d.remaining())
                .ok_or(Error::new("invalid MIR section bounds"))?;
            let end = start
                .checked_add(size)
                .filter(|&end| end <= source.bytes.len())
                .ok_or(Error::new("invalid MIR chunk bounds"))?;
            spans.push(start..end);
        }
        chunks.push(Chunk {
            first,
            count: (count - start).min(GROUP_SIZE),
            bytes: d.read(size)?,
        });
    }
    let cx = Context {
        items,
        sigs,
        budget: d.share_allocation_budget(),
        check_types: !d.mir_typing_staged(),
    };
    if let Some(source) = source {
        let functions: Vec<_> = source
            .functions
            .iter()
            .copied()
            .map(FunctionId::from_raw)
            .filter(|&id| items.functions.get(id).is_some_and(|def| def.has_body))
            .collect();
        if functions.len() == count
            && chunks
                .iter()
                .zip(functions.chunks(GROUP_SIZE))
                .all(|(chunk, expected)| chunk.first == expected[0])
        {
            let archive = mir.deferred_functions.archives.len();
            for (ordinal, &id) in functions.iter().enumerate() {
                if mir.bodies.contains(id)
                    || mir
                        .deferred_functions
                        .locations
                        .insert(id, (archive, ordinal))
                        .is_some()
                {
                    return Err(Error::new("duplicate imported MIR body"));
                }
            }
            mir.deferred_functions.archives.push(DeferredArchive {
                bytes: source.bytes.clone(),
                ids: d.ids.clone(),
                budget: cx.budget,
                functions,
                chunks: spans
                    .into_iter()
                    .map(|payload| DeferredChunk {
                        payload,
                        result: OnceLock::new(),
                    })
                    .collect(),
            });
            return Ok(());
        }
    }
    let mut previous = None;
    let mut append = |id, body| {
        if previous.is_some_and(|previous| previous >= id) || mir.bodies.get(id).is_some() {
            return Err(Error::new("duplicate or unordered imported MIR body"));
        }
        mir.bodies.insert(id, body);
        previous = Some(id);
        Ok(())
    };
    let workers = workers.min(count / MIN_ROWS_PER_WORKER);
    if workers <= 1 {
        return cx.decode(&chunks, &mut d.ids, validated_types, append);
    }
    std::thread::scope(|scope| {
        let mut pending = Vec::new();
        let cx = &cx;
        let ids = &d.ids;
        for group in chunks.chunks(chunks.len().div_ceil(workers)) {
            let worker = std::thread::Builder::new()
                .stack_size(8 * 1024 * 1024)
                .spawn_scoped(scope, move || {
                    let _fields = flux_solve::remember_field_types(sigs);
                    let _queries = flux_solve::remember_solver_queries(items, sigs);
                    let mut rows = FunctionRows::new();
                    let mut ids = ids.clone();
                    cx.decode(group, &mut ids, &mut FxHashSet::default(), |id, body| {
                        rows.push((id, body));
                        Ok(())
                    })?;
                    Ok::<_, Error>(rows)
                })
                .map_err(|_| Error::new("cannot start MIR import worker"))?;
            pending.push(worker);
        }
        let joined: Vec<_> = pending.into_iter().map(|worker| worker.join()).collect();
        for result in joined {
            for (id, body) in result.map_err(|_| Error::new("MIR import worker panicked"))?? {
                append(id, body)?;
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::imported::{FunctionSummary, ImportedFunctionSummaries, ImportedMir};
    use std::fmt::Write;

    #[test]
    fn indexed_function_rows_match_serial_metadata() {
        check_indexed_function_rows(2 * MIN_ROWS_PER_WORKER + 1);
    }

    #[test]
    fn indexed_function_rows_support_archives_below_parallel_cutoff() {
        check_indexed_function_rows(MIN_INDEXED_FUNCTIONS);
    }

    fn check_indexed_function_rows(generated: usize) {
        let mut source = String::from(
            "fn main() {} fn leaf(x: u32) -> u32 { x } fn promoted() {} async fn future() -> u32 { 3 }",
        );
        for index in 0..generated {
            writeln!(
                &mut source,
                "fn body_{index}(x: u32) -> u32 {{ leaf(x) + {index} }}"
            )
            .unwrap();
        }
        let (_, _, hir) = flux_hir::lower_text("indexed-rows.rs", &source);
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mut mir = crate::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);

        let mut ids = IdMap::default();
        let mut functions = Vec::new();
        for (id, _) in mir.bodies.iter().filter(|(id, _)| hir.is_program(*id)) {
            ids.mark_owned(flux_hir::flib::IdKind::Function, id.into_raw());
            functions.push(id.into_raw());
        }
        let owned = |name| {
            functions
                .iter()
                .copied()
                .map(FunctionId::from_raw)
                .find(|&id| hir.items.functions[id].name.as_str() == name)
                .unwrap()
        };
        mir.bodies
            .get_mut(owned("main"))
            .unwrap()
            .promoted
            .push(crate::eval::Value::Function(flux_ty::Instance {
                function: owned("promoted"),
                args: &[],
            }));
        let mut summary = Encoder::with_ids(ids.clone());
        FunctionSummary::collect(&mir, &hir.items, &functions)
            .unwrap()
            .encode(&mut summary)
            .unwrap();
        let summary = summary.into_bytes();
        let mut plain = Encoder::with_ids(ids.clone());
        mir.encode_flib(&mut plain).unwrap();
        let expected = plain.into_bytes();
        let mut indexed = Encoder::with_ids(ids.clone());
        mir.encode_indexed_flib(&mut indexed).unwrap();
        let bytes = indexed.into_bytes();
        let mut header = Decoder::new(&bytes);
        assert_eq!(u32::decode(&mut header), Ok(HEADER));
        let _ = crate::PanicStrategy::decode(&mut header).unwrap();
        let count = header.length().unwrap();
        assert_eq!(count, functions.len());
        let index_offset = bytes.len() - header.remaining();

        for workers in [1, 4] {
            let mut decoded = FileMir::default();
            let mut reader = Decoder::with_ids(&bytes, ids.clone());
            decoded
                .decode_flib_with_workers(&mut reader, &hir.items, &typed.sigs, workers)
                .unwrap();
            reader.finish().unwrap();
            let mut output = Encoder::with_ids(ids.clone());
            decoded.encode_flib(&mut output).unwrap();
            assert_eq!(output.into_bytes(), expected);

            let mut facts = Decoder::with_ids(&summary, ids.clone());
            let facts =
                FunctionSummary::decode(&mut facts, &hir.items, &decoded, &typed.sigs, &functions)
                    .unwrap();
            let mut summaries = ImportedFunctionSummaries::default();
            summaries.append(Some(facts), &functions);
            let eager = ImportedMir::new(decoded, hir.items.counts()).unwrap();
            let roots = eager.required_functions(&hir.items, &typed.sigs).functions;
            assert!(roots.contains(&owned("leaf")));
            assert!(roots.contains(&owned("promoted")));
            let coroutines = eager.coroutine_owners(&hir.items);
            assert!(coroutines.contains(&flux_ty::ClosureOwner::Function(owned("future"))));
            let indexed = eager.with_function_summaries(summaries);
            assert_eq!(
                indexed
                    .required_functions(&hir.items, &typed.sigs)
                    .functions,
                roots
            );
            assert_eq!(indexed.coroutine_owners(&hir.items), coroutines);
        }

        let load_deferred = |bytes: Vec<u8>| {
            let bytes = Arc::new(bytes);
            let mut mir = FileMir::default();
            let mut reader = Decoder::with_ids(&bytes, ids.clone());
            mir.decode_flib_for_import(
                &mut reader,
                &hir.items,
                &typed.sigs,
                4,
                Some(DeferredFunctionSource {
                    bytes: bytes.clone(),
                    section_end: bytes.len(),
                    functions: &functions,
                }),
            )
            .unwrap();
            reader.finish().unwrap();
            mir
        };
        let deferred = load_deferred(bytes.clone());
        assert_eq!(
            deferred.deferred_functions.chunk_counts(),
            (0, count.div_ceil(GROUP_SIZE))
        );
        let last = FunctionId::from_raw(*functions.last().unwrap());
        let body = deferred
            .function_body(&hir.items, &typed.sigs, last)
            .unwrap()
            .unwrap();
        assert_eq!(body.owner, Some(last));
        assert_eq!(deferred.deferred_functions.chunk_counts().0, 1);
        let again = deferred
            .function_body(&hir.items, &typed.sigs, last)
            .unwrap()
            .unwrap();
        assert!(std::ptr::eq(body, again));
        assert_eq!(deferred.deferred_functions.chunk_counts().0, 1);

        let mut damaged_payload = bytes.clone();
        let first = u32::from_le_bytes(bytes[index_offset..index_offset + 4].try_into().unwrap());
        damaged_payload[index_offset + 8..index_offset + 12]
            .copy_from_slice(&(first + 1).to_le_bytes());
        let deferred = load_deferred(damaged_payload);
        assert_eq!(deferred.deferred_functions.chunk_counts().0, 0);
        assert!(
            deferred
                .function_body(&hir.items, &typed.sigs, FunctionId::from_raw(first))
                .is_err()
        );
        assert_eq!(
            deferred.deferred_error(),
            Some(&Error::new("invalid MIR chunk function order"))
        );

        let mut corrupt = bytes;
        let key = u32::from_le_bytes(corrupt[index_offset..index_offset + 4].try_into().unwrap());
        corrupt[index_offset..index_offset + 4].copy_from_slice(&(key + 1).to_le_bytes());
        let mut reader = Decoder::with_ids(&corrupt, ids);
        let result =
            FileMir::default().decode_flib_with_workers(&mut reader, &hir.items, &typed.sigs, 4);
        assert_eq!(result, Err(Error::new("invalid MIR chunk function order")));
    }
}
