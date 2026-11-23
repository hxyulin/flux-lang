//! Independently encoded data tables in package MIR.

use super::{DeferredFunctionSource, boundary, decode_map_key};
use crate::eval::{ConstValues, Value};
use flux_hir::flib::{AllocationBudget, Decode, Decoder, Encode, Encoder, Error, IdMap};
use flux_hir::{Const, ConstId, ItemTree};
use flux_ty::Signatures;
use rustc_hash::FxHashSet;
use std::ops::Range;
use std::sync::{Arc, OnceLock};

pub(super) const HEADER: u32 = u32::MAX;
const MIN_TABLES: usize = 128;
const MIN_PAYLOAD: usize = 64 * 1024;

type ValueRow<'a> = (ConstId, &'a Value);

// These tables have no static, function or vtable references. Discovering
// codegen roots can therefore leave rolled-up data tables unread.
fn is_table(value: &Value, seen: &mut FxHashSet<(usize, bool)>, depth: u32) -> bool {
    if depth > 128 {
        return false;
    }
    match value {
        Value::Int { .. } | Value::Bool(_) | Value::Float { .. } => true,
        Value::Ref(value) => {
            let key = (Arc::as_ptr(value) as usize, false);
            seen.contains(&key) || is_table(value, seen, depth + 1) && seen.insert(key)
        }
        Value::Aggregate(values) => {
            let key = (values.as_ptr() as usize, true);
            seen.contains(&key)
                || values.iter().all(|value| is_table(value, seen, depth + 1)) && seen.insert(key)
        }
        _ => false,
    }
}

pub(super) fn partition<'a>(rows: &mut Vec<ValueRow<'a>>, items: &ItemTree) -> Vec<ValueRow<'a>> {
    if rows.len() < MIN_TABLES {
        return Vec::new();
    }
    let mut seen = FxHashSet::default();
    let mut tables = Vec::new();
    rows.retain(|&(id, value)| {
        if items.consts[id].is_static.is_none() && is_table(value, &mut seen, 0) {
            tables.push((id, value));
            false
        } else {
            true
        }
    });
    tables
}

pub(super) fn encode(rows: &[ValueRow<'_>], e: &mut Encoder) -> Result<(), Error> {
    rows.len().encode(e)?;
    for (id, _) in rows {
        id.encode(e)?;
    }
    e.independent_records(|records| {
        for (_, value) in rows {
            value.encode(records)?;
        }
        Ok(())
    })
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DeferredValues(Vec<Arc<ValueGroup>>);

#[derive(Debug)]
struct ValueGroup {
    bytes: Arc<Vec<u8>>,
    payload: Range<usize>,
    keys: Vec<ConstId>,
    ids: IdMap,
    budget: AllocationBudget,
    values: OnceLock<Result<Vec<Value>, Error>>,
}

impl DeferredValues {
    pub(crate) fn keys(&self) -> impl Iterator<Item = ConstId> + '_ {
        self.0.iter().flat_map(|group| group.keys.iter().copied())
    }

    pub(crate) fn contains(&self, id: ConstId) -> bool {
        self.0
            .iter()
            .any(|group| group.keys.binary_search(&id).is_ok())
    }

    pub(crate) fn get(
        &self,
        id: ConstId,
        items: &ItemTree,
        sigs: &Signatures,
    ) -> Result<Option<&Value>, Error> {
        for group in &self.0 {
            if let Ok(index) = group.keys.binary_search(&id) {
                let values = group.values.get_or_init(|| {
                    let mut d =
                        Decoder::with_ids(&group.bytes[group.payload.clone()], group.ids.clone())
                            .with_allocation_budget(group.budget.clone());
                    read_values(&group.keys, &mut d, items, sigs)
                });
                return values
                    .as_ref()
                    .map(|values| Some(&values[index]))
                    .map_err(Clone::clone);
            }
        }
        Ok(None)
    }

    pub(crate) fn error(&self) -> Option<&Error> {
        self.0
            .iter()
            .find_map(|group| group.values.get()?.as_ref().err())
    }

    pub(crate) fn extend(&mut self, other: &Self) {
        self.0.extend(other.0.iter().cloned());
    }
}

pub(super) fn decode(
    values: &mut ConstValues,
    d: &mut Decoder<'_>,
    items: &ItemTree,
    sigs: &Signatures,
    source: Option<DeferredFunctionSource<'_>>,
) -> Result<(), Error> {
    let n = d.length()?;
    let mut keys = Vec::with_capacity(n);
    for _ in 0..n {
        let id = decode_map_key::<Const>(d)?;
        if keys.last().is_some_and(|&last| last >= id)
            || values.contains_item(id)
            || items
                .consts
                .get(id)
                .is_none_or(|def| def.is_static.is_some())
            || sigs.consts.get(id).is_none()
        {
            return Err(Error::new("invalid indexed constant value"));
        }
        keys.push(id);
    }
    let length = u32::decode(d)? as usize;
    let remaining = d.remaining();
    let records = d.read(length)?;
    let budget = d.share_allocation_budget();
    if let Some(source) = source.filter(|_| length >= MIN_PAYLOAD) {
        let start = source
            .section_end
            .checked_sub(remaining)
            .ok_or(Error::new("invalid constant value section bounds"))?;
        let end = start
            .checked_add(length)
            .filter(|&end| end <= source.bytes.len())
            .ok_or(Error::new("invalid constant value group bounds"))?;
        values.deferred.0.push(Arc::new(ValueGroup {
            bytes: source.bytes,
            payload: start..end,
            keys,
            ids: d.ids.clone(),
            budget,
            values: OnceLock::new(),
        }));
    } else {
        let mut group = Decoder::with_ids(records, d.ids.clone()).with_allocation_budget(budget);
        let decoded = read_values(&keys, &mut group, items, sigs)?;
        for (id, value) in keys.into_iter().zip(decoded) {
            values.items.insert(id, value);
        }
    }
    Ok(())
}

fn read_values(
    keys: &[ConstId],
    d: &mut Decoder<'_>,
    items: &ItemTree,
    sigs: &Signatures,
) -> Result<Vec<Value>, Error> {
    let mut seen = FxHashSet::default();
    let mut values = Vec::with_capacity(keys.len());
    for &id in keys {
        let value = Value::decode(d)?;
        if !is_table(&value, &mut seen, 0) {
            return Err(Error::new("invalid indexed data table"));
        }
        let ty = *sigs
            .consts
            .get(id)
            .ok_or(Error::new("missing constant signature"))?;
        boundary::value_for(&value, ty, items, sigs, 0)?;
        values.push(value);
    }
    d.finish()?;
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_ty::IntTy;

    #[test]
    fn package_tables_validate_on_first_read() {
        let (_, _, hir) =
            flux_hir::lower_text("tables.rs", "pub const TABLE: &[u16] = &[7; 40000];");
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let id = hir
            .items
            .consts
            .iter()
            .find(|(_, def)| {
                hir.items.modules[def.loc.module].krate == hir.items.main_crate()
                    && def.name.is_some_and(|name| name.as_str() == "TABLE")
            })
            .unwrap()
            .0;
        let table = |ty| {
            Value::Ref(Arc::new(Value::Aggregate(
                vec![Value::Int { bits: 7, ty }; 40_000].into(),
            )))
        };
        let import = |value: &Value, sigs: &Signatures| {
            let mut e = Encoder::new();
            encode(&[(id, value)], &mut e).unwrap();
            let bytes = Arc::new(e.into_bytes());
            let mut d = Decoder::new(&bytes);
            let mut data = ConstValues::default();
            decode(
                &mut data,
                &mut d,
                &hir.items,
                sigs,
                Some(DeferredFunctionSource {
                    bytes: bytes.clone(),
                    section_end: bytes.len(),
                    functions: &[],
                }),
            )
            .unwrap();
            d.finish().unwrap();
            assert!(data.contains_item(id));
            assert!(data.items.get(id).is_none());
            assert!(data.deferred.0[0].values.get().is_none());
            data
        };
        let valid = table(IntTy::U16);
        let data = import(&valid, &typed.sigs);
        let copied = data.clone();
        let loaded = data.get_checked(id, &[], &hir.items, &typed.sigs).unwrap();
        assert_eq!(loaded, Some(&valid));
        let shared = copied
            .get_checked(id, &[], &hir.items, &typed.sigs)
            .unwrap();
        assert!(std::ptr::eq(loaded.unwrap(), shared.unwrap()));

        let invalid = table(IntTy::I16);
        let data = import(&invalid, &typed.sigs);
        let error = data
            .get_checked(id, &[], &hir.items, &typed.sigs)
            .unwrap_err();
        assert_eq!(data.deferred.error(), Some(&error));
    }
}
