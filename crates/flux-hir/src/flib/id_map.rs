//! Reference mappings between metadata and process arenas.

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use super::identity::ITEM_KINDS;
use super::{Error, IdKind};

pub(super) const SLOT_SHIFT: u32 = 24;
pub(super) const SLOT_SIZE: u32 = 1 << SLOT_SHIFT;

#[derive(Debug, Clone)]
enum DecodeRow {
    Span { first: u32, len: u32 },
    Sparse(Box<[u32]>),
}

#[test]
fn builtin_identity_preserves_the_unowned_file_sentinel() {
    let identity = IdMap::builtin_identity(&[2; 11]);
    assert_eq!(identity.get(IdKind::Function, 1), Ok(1));
    assert!(identity.get(IdKind::Function, 2).is_err());
    assert!(identity.contains_owned(IdKind::Function, 1));
    assert_eq!(identity.get(IdKind::File, u32::MAX), Ok(u32::MAX));
    assert!(!identity.contains_owned(IdKind::File, u32::MAX));

    let mut expanded = identity.inverse().unwrap();
    expanded.insert(IdKind::Function, 1, 3);
    assert_eq!(expanded.get(IdKind::Function, 1), Ok(3));
    expanded.extend(&identity);
    assert_eq!(expanded.get(IdKind::Function, 1), Ok(1));
    assert!(!expanded.contains_owned(IdKind::File, u32::MAX));
}

impl DecodeRow {
    fn sorted(indices: &[u32]) -> Self {
        let first = indices.first().copied().unwrap_or(0);
        let len = indices.len() as u32;
        if indices.last().is_none_or(|&last| last - first == len - 1) {
            Self::Span { first, len }
        } else {
            Self::Sparse(indices.into())
        }
    }

    fn get(&self, local: u32) -> Option<u32> {
        match self {
            Self::Span { first, len } => (local < *len).then(|| first + local),
            Self::Sparse(indices) => indices.get(local as usize).copied(),
        }
    }

    fn contains(&self, index: u32) -> bool {
        match self {
            Self::Span { first, len } => index.wrapping_sub(*first) < *len,
            Self::Sparse(indices) => indices.binary_search(&index).is_ok(),
        }
    }
}

#[derive(Debug, Clone)]
struct DecodeSlot {
    krate: u32,
    rows: [DecodeRow; 10],
}

impl DecodeSlot {
    fn items(&self) -> impl Iterator<Item = (IdKind, u32, u32)> + '_ {
        ITEM_KINDS
            .into_iter()
            .zip(&self.rows)
            .flat_map(|(kind, row)| {
                (0..).map_while(move |local| Some((kind, local, row.get(local)?)))
            })
    }
}

#[derive(Debug, Clone)]
enum CompactMappings {
    Slots(Vec<Option<DecodeSlot>>),
    BuiltinIdentity([u32; 11]),
}

/// Explicit mappings between process and artifact-local indices.
#[derive(Debug, Clone, Default)]
pub struct IdMap {
    entries: FxHashMap<(IdKind, u32), u32>,
    // Independent section and MIR chunk readers share these immutable rows.
    compact: Option<Arc<CompactMappings>>,
    strict: bool,
    owned: FxHashSet<(IdKind, u32)>,
    source_ranges: Vec<(u32, u32, u32)>,
}
impl IdMap {
    /// Creates a map that rejects references without an explicit mapping.
    pub fn strict() -> Self {
        Self {
            strict: true,
            ..Self::default()
        }
    }

    pub(super) fn builtin_identity(counts: &[usize; 11]) -> Self {
        Self {
            compact: Some(Arc::new(CompactMappings::BuiltinIdentity(
                counts.map(|count| count as u32),
            ))),
            ..Self::strict()
        }
    }

    /// Records one reference mapping.
    pub fn insert(&mut self, kind: IdKind, from: u32, to: u32) {
        self.expand_compact();
        self.entries.insert((kind, from), to);
    }

    /// Installs a vacant wire slot whose rows are strictly increasing.
    pub(super) fn add_decode_slot(&mut self, slot: u32, krate: u32, rows: [&[u32]; 10]) -> bool {
        let slot = slot as usize;
        if !self.entries.is_empty()
            || self.identity_limits().is_some()
            || self.decode_slot(slot).is_some()
        {
            return false;
        }
        let compact = self
            .compact
            .get_or_insert_with(|| Arc::new(CompactMappings::Slots(Vec::new())));
        let CompactMappings::Slots(slots) = Arc::make_mut(compact) else {
            unreachable!("vacant artifact slot requires an artifact map");
        };
        if slots.len() <= slot {
            slots.resize_with(slot + 1, || None);
        }
        slots[slot] = Some(DecodeSlot {
            krate,
            rows: rows.map(DecodeRow::sorted),
        });
        true
    }

    fn decode_slot(&self, slot: usize) -> Option<&DecodeSlot> {
        self.slots().get(slot)?.as_ref()
    }

    fn slots(&self) -> &[Option<DecodeSlot>] {
        match self.compact.as_deref() {
            Some(CompactMappings::Slots(slots)) => slots,
            _ => &[],
        }
    }

    fn identity_limits(&self) -> Option<&[u32; 11]> {
        match self.compact.as_deref() {
            Some(CompactMappings::BuiltinIdentity(limits)) => Some(limits),
            _ => None,
        }
    }

    fn decode_get(&self, kind: IdKind, wire: u32) -> Option<u32> {
        if kind == IdKind::Crate {
            return Some(self.decode_slot(wire as usize)?.krate);
        }
        let slot = self.decode_slot((wire >> SLOT_SHIFT) as usize)?;
        slot.rows[kind as usize - 1].get(wire & (SLOT_SIZE - 1))
    }

    fn decode_entries(
        slots: &[Option<DecodeSlot>],
    ) -> impl Iterator<Item = ((IdKind, u32), u32)> + '_ {
        slots.iter().enumerate().flat_map(|(index, slot)| {
            let index = index as u32;
            slot.iter().flat_map(move |slot| {
                std::iter::once(((IdKind::Crate, index), slot.krate)).chain(slot.items().map(
                    move |(kind, local, value)| ((kind, (index << SLOT_SHIFT) | local), value),
                ))
            })
        })
    }

    fn decode_owned(slots: &[Option<DecodeSlot>]) -> impl Iterator<Item = (IdKind, u32)> + '_ {
        slots
            .first()
            .and_then(Option::as_ref)
            .into_iter()
            .flat_map(|slot| slot.items().map(|(kind, _, value)| (kind, value)))
    }

    fn identity_owned(&self) -> impl Iterator<Item = (IdKind, u32)> + '_ {
        self.identity_limits().into_iter().flat_map(|limits| {
            std::iter::once(IdKind::Crate)
                .chain(ITEM_KINDS)
                .zip(limits)
                .flat_map(|(kind, &limit)| (0..limit).map(move |raw| (kind, raw)))
        })
    }

    fn owned_entries(&self) -> impl Iterator<Item = (IdKind, u32)> + '_ {
        self.owned
            .iter()
            .copied()
            .chain(Self::decode_owned(self.slots()))
            .chain(self.identity_owned())
    }

    // Explicit insertion can overwrite a packed mapping. Keep the same merge
    // and inversion semantics for callers that use the general-purpose API.
    fn expand_compact(&mut self) {
        if let Some(compact) = self.compact.take() {
            let old = Self {
                compact: Some(compact),
                ..Self::default()
            };
            self.entries.extend(old.mappings());
            self.owned.extend(old.owned_entries());
        }
    }
    /// Registers the process coordinates of an included diagnostic source.
    pub fn add_source_range(&mut self, file: u32, start: u32, end: u32) {
        self.source_ranges.push((file, start, end));
    }
    pub(super) fn source_range(&self, range: flux_syntax::TextRange) -> Option<(u32, u32)> {
        self.source_ranges.iter().find_map(|&(file, start, end)| {
            (range.start() >= start && range.end() <= end).then_some((file, start))
        })
    }
    pub(super) fn source_bounds(&self, file: u32) -> Option<(u32, u32)> {
        self.source_ranges
            .iter()
            .find_map(|&(found, start, end)| (found == file).then_some((start, end)))
    }
    pub fn is_strict(&self) -> bool {
        self.strict
    }
    pub fn mark_owned(&mut self, kind: IdKind, index: u32) {
        self.owned.insert((kind, index));
    }
    pub fn contains_owned(&self, kind: IdKind, index: u32) -> bool {
        self.owned.contains(&(kind, index))
            || self
                .identity_limits()
                .is_some_and(|limits| index < limits[kind as usize])
            || kind != IdKind::Crate
                && self
                    .decode_slot(0)
                    .is_some_and(|slot| slot.rows[kind as usize - 1].contains(index))
    }
    pub fn extend(&mut self, other: &Self) {
        self.expand_compact();
        self.entries.extend(other.mappings());
        self.owned.extend(other.owned_entries());
        self.source_ranges
            .extend(other.source_ranges.iter().copied());
    }
    /// Resolves a reference, rejecting missing mappings in a strict map.
    pub fn get(&self, kind: IdKind, from: u32) -> Result<u32, Error> {
        let found = match self.compact.as_deref() {
            Some(CompactMappings::Slots(_)) => self.decode_get(kind, from),
            Some(CompactMappings::BuiltinIdentity(limits)) => (from < limits[kind as usize]
                || (kind == IdKind::File && from == u32::MAX))
                .then_some(from),
            None => self.entries.get(&(kind, from)).copied(),
        };
        match found {
            Some(to) => Ok(to),
            None if self.strict => Err(Error("unmapped item reference")),
            None => Ok(from),
        }
    }
    /// Returns the inverse mapping, rejecting ambiguous destination indices.
    pub fn inverse(&self) -> Result<Self, Error> {
        let mut result = Self::strict();
        for ((kind, from), to) in self.mappings() {
            if result.entries.insert((kind, to), from).is_some() {
                return Err(Error("duplicate item reference"));
            }
        }
        result.owned.extend(self.owned_entries());
        result.source_ranges = self.source_ranges.clone();
        Ok(result)
    }

    fn mappings(&self) -> impl Iterator<Item = ((IdKind, u32), u32)> + '_ {
        self.entries
            .iter()
            .map(|(&key, &value)| (key, value))
            .chain(Self::decode_entries(self.slots()))
            .chain(self.identity_owned().map(|(kind, raw)| ((kind, raw), raw)))
            .chain(
                self.identity_limits()
                    .map(|_| ((IdKind::File, u32::MAX), u32::MAX)),
            )
    }
}
