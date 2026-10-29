//! Ownership scans and reference-table construction.

use super::{Error, IdKind, IdMap};
use crate::{CrateId, ItemTree};

/// Process indices of one crate in deterministic per-arena order.
#[derive(Debug, Clone, Default)]
pub struct CrateIndices {
    pub modules: Vec<u32>,
    pub functions: Vec<u32>,
    pub structs: Vec<u32>,
    pub traits: Vec<u32>,
    pub impls: Vec<u32>,
    pub consts: Vec<u32>,
    pub type_aliases: Vec<u32>,
    pub macros: Vec<u32>,
    pub macro_scopes: Vec<u32>,
    pub files: Vec<u32>,
    /// File IDs and half-open process coordinate intervals of included sources.
    pub source_ranges: Vec<(u32, u32, u32)>,
}
impl CrateIndices {
    /// Finds crate ownership without assuming contiguous process arena slices.
    pub fn collect(items: &ItemTree, krate: CrateId) -> Self {
        let owns = |module| items.modules[module].krate == krate;
        let mut result = Self {
            modules: items
                .modules
                .iter()
                .filter(|(_, x)| x.krate == krate)
                .map(|(id, _)| id.into_raw())
                .collect(),
            functions: items
                .functions
                .iter()
                .filter(|(_, x)| owns(x.loc.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            structs: items
                .structs
                .iter()
                .filter(|(_, x)| owns(x.loc.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            traits: items
                .traits
                .iter()
                .filter(|(_, x)| owns(x.loc.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            impls: items
                .impls
                .iter()
                .filter(|(_, x)| owns(x.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            consts: items
                .consts
                .iter()
                .filter(|(_, x)| owns(x.loc.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            type_aliases: items
                .type_aliases
                .iter()
                .filter(|(_, x)| owns(x.loc.module))
                .map(|(id, _)| id.into_raw())
                .collect(),
            macros: items
                .macros
                .iter()
                .filter(|(_, x)| x.krate == krate)
                .map(|(id, _)| id.into_raw())
                .collect(),
            macro_scopes: items
                .macro_scopes
                .iter()
                .filter(|(_, x)| items.macros[x.def].krate == krate)
                .map(|(id, _)| id.into_raw())
                .collect(),
            files: Vec::new(),
            source_ranges: Vec::new(),
        };
        let mut files = std::collections::BTreeSet::new();
        for &index in &result.modules {
            files.insert(items.modules[flux_arena::Idx::from_raw(index)].file.0);
        }
        for &index in &result.functions {
            files.insert(items.functions[flux_arena::Idx::from_raw(index)].loc.file.0);
        }
        for &index in &result.consts {
            files.insert(items.consts[flux_arena::Idx::from_raw(index)].loc.file.0);
        }
        for &index in &result.macros {
            files.insert(items.macros[flux_arena::Idx::from_raw(index)].file.0);
        }
        if let Some(retained) = items.artifact_files.get(&krate) {
            files.extend(retained.iter().map(|id| id.0));
        }
        loop {
            let old = files.len();
            for (&from, &to) in &items.expansion_files {
                if files.contains(&from.0) {
                    files.insert(to.0);
                }
            }
            for (&(from, _), expansion) in &items.body_expansions {
                if files.contains(&from.0) {
                    files.insert(expansion.file.0);
                }
            }
            if files.len() == old {
                break;
            }
        }
        result.files = files.into_iter().collect();
        for &index in &result.files {
            if let Some(source) = items.files.get(&crate::FileId(index))
                && source.range_base != 0
            {
                result.source_ranges.push((
                    index,
                    source.range_base,
                    source.range_base + source.text().len() as u32,
                ));
            }
        }
        result
    }
    /// Visits each independently indexed ownership table.
    pub fn tables(&self) -> [(IdKind, &[u32]); 10] {
        [
            (IdKind::Module, &self.modules),
            (IdKind::Function, &self.functions),
            (IdKind::Struct, &self.structs),
            (IdKind::Trait, &self.traits),
            (IdKind::Impl, &self.impls),
            (IdKind::Const, &self.consts),
            (IdKind::TypeAlias, &self.type_aliases),
            (IdKind::Macro, &self.macros),
            (IdKind::MacroScope, &self.macro_scopes),
            (IdKind::File, &self.files),
        ]
    }
    /// Adds process-to-wire mappings. Slot zero is the artifact's own crate.
    pub fn add_mapping(&self, map: &mut IdMap, krate: CrateId, slot: u32) -> Result<(), Error> {
        if slot > 255 {
            return Err(Error("too many artifact dependencies"));
        }
        map.insert(IdKind::Crate, krate.0, slot);
        for (kind, indices) in self.tables() {
            if indices.len() > 0x0100_0000 {
                return Err(Error("too many artifact items"));
            }
            for (local, &source) in indices.iter().enumerate() {
                map.insert(kind, source, (slot << 24) | local as u32);
                if slot == 0 {
                    map.mark_owned(kind, source);
                }
            }
        }
        for &(file, start, end) in &self.source_ranges {
            map.add_source_range(file, start, end);
        }
        Ok(())
    }
    /// Adds wire-to-process references for an already loaded dependency.
    pub fn add_decode_mapping(
        &self,
        map: &mut IdMap,
        krate: CrateId,
        slot: u32,
    ) -> Result<(), Error> {
        let mut encoder = IdMap::strict();
        self.add_mapping(&mut encoder, krate, slot)?;
        map.extend(&encoder.inverse()?);
        Ok(())
    }
}
