//! Independently decoded closure MIR groups for large crate archives.

use super::{ClosureRow, validate_closure_body};
use crate::{Body, FileMir};
use flux_hir::ItemTree;
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error};
use flux_ty::Signatures;

pub(super) const HEADER: u32 = u32::MAX;
pub(super) const MIN_INDEXED_CLOSURES: usize = 512;
const GROUP_SIZE: usize = 128;

pub(super) fn encode(
    e: &mut Encoder,
    rows: &[(&(flux_ty::ClosureId, flux_ty::ClosureKind), &Body)],
) -> Result<(), Error> {
    HEADER.encode(e)?;
    rows.len().encode(e)?;
    for group in rows.chunks(GROUP_SIZE) {
        e.independent_records(|record| {
            for (key, body) in group {
                key.encode(record)?;
                body.encode(record)?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

pub(super) fn decode(
    mir: &mut FileMir,
    d: &mut Decoder<'_>,
    items: &ItemTree,
    sigs: &Signatures,
    workers: usize,
) -> Result<(), Error> {
    let count = d.length()?;
    if count < MIN_INDEXED_CLOSURES {
        return Err(Error::new("invalid indexed closure count"));
    }
    let groups = (0..count)
        .step_by(GROUP_SIZE)
        .map(|start| Ok((d.blob()?, (count - start).min(GROUP_SIZE))))
        .collect::<Result<Vec<_>, Error>>()?;
    let budget = d.share_allocation_budget();
    let check_types = !d.mir_typing_staged();
    let ids = &d.ids;
    let decode = |groups: &[(&[u8], usize)]| {
        let _fields = flux_solve::remember_field_types(sigs);
        let _queries = flux_solve::remember_solver_queries(items, sigs);
        let mut types = rustc_hash::FxHashSet::default();
        let mut rows = Vec::new();
        let result = (|| {
            for &(bytes, count) in groups {
                let mut part =
                    Decoder::with_ids(bytes, ids.clone()).with_allocation_budget(budget.clone());
                for _ in 0..count {
                    let key = <(flux_ty::ClosureId, flux_ty::ClosureKind)>::decode(&mut part)?;
                    let body = Body::decode(&mut part)?;
                    validate_closure_body(key, &body, ids, items, sigs, check_types, &mut types)?;
                    rows.push((key, body));
                }
                part.finish()?;
            }
            Ok(())
        })();
        (rows, result)
    };
    let mut append = |(rows, result): (Vec<ClosureRow>, Result<(), Error>)| {
        // Preserve an earlier duplicate-key error in the decoded prefix.
        for (key, body) in rows {
            if mir.closures.insert(key, body).is_some() {
                return Err(Error::new("duplicate imported closure"));
            }
        }
        result
    };
    let workers = workers.min(count / 256).max(1);
    let mut batches = groups.chunks(groups.len().div_ceil(workers));
    let first = batches.next().expect("nonempty indexed closure groups");
    std::thread::scope(|scope| {
        let mut helpers = Vec::new();
        for batch in batches {
            helpers.push(
                std::thread::Builder::new()
                    .stack_size(8 * 1024 * 1024)
                    .spawn_scoped(scope, move || decode(batch))
                    .map_err(|_| batch),
            );
        }
        append(decode(first))?;
        for worker in helpers {
            match worker {
                Ok(worker) => append(worker.join().expect("closure decoder panicked"))?,
                Err(batch) => append(decode(batch))?,
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write;

    #[test]
    fn indexed_closures_preserve_records_and_reject_duplicates() {
        let mut source = String::new();
        for index in 0..=MIN_INDEXED_CLOSURES / 3 {
            writeln!(
                source,
                "pub fn f{index}() -> u32 {{ let add = |x: u32| x + 1; add({index}) }}"
            )
            .unwrap();
        }
        let (_, _, hir) = flux_hir::lower_text("closures.rs", &source);
        let typed = crate::check_file(&hir);
        let original = crate::build_file(&hir, &typed);
        assert!(
            original.diagnostics.is_empty(),
            "{:?}",
            original.diagnostics
        );
        let rows: Vec<_> = original
            .closures
            .iter()
            .filter(|((id, _), _)| match id.owner {
                flux_ty::ClosureOwner::Function(f) => {
                    hir.items.modules[hir.items.functions[f].loc.module].krate
                        == hir.items.main_crate()
                }
                flux_ty::ClosureOwner::Const(_) => false,
            })
            .collect();
        assert!(rows.len() >= MIN_INDEXED_CLOSURES);
        let mut e = Encoder::new();
        encode(&mut e, &rows).unwrap();
        let bytes = e.into_bytes();
        let restore = |mir: &mut FileMir, workers| {
            let mut d = Decoder::new(&bytes);
            assert_eq!(u32::decode(&mut d), Ok(HEADER));
            decode(mir, &mut d, &hir.items, &typed.sigs, workers)?;
            d.finish()
        };
        for workers in [1, 8] {
            let mut restored = FileMir::default();
            restore(&mut restored, workers).unwrap();
            assert_eq!(restored.closures.len(), rows.len());
            let restored_rows: Vec<_> = rows
                .iter()
                .map(|&(key, _)| (key, &restored.closures[key]))
                .collect();
            let mut encoded = Encoder::new();
            encode(&mut encoded, &restored_rows).unwrap();
            assert_eq!(encoded.into_bytes(), bytes);
            assert_eq!(
                restore(&mut restored, workers).unwrap_err(),
                Error::new("duplicate imported closure")
            );
        }
    }
}
