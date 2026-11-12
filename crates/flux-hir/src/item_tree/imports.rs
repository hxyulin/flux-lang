//! Imports in source order, indexed for lookup during item collection.

use std::ops::Deref;

use flux_intern::Symbol;
use rustc_hash::FxHashMap;

use super::{ModuleId, Use, UseKind};

/// Append-only imports with read-only access to their source order.
/// Keeping mutation here prevents the name index from becoming stale.
#[derive(Debug, Clone, Default)]
pub struct Uses {
    values: Vec<Use>,
    names: FxHashMap<(ModuleId, Symbol), Vec<usize>>,
    globs: FxHashMap<ModuleId, Vec<usize>>,
}

impl Uses {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, import: Use) {
        if let UseKind::Named(name) | UseKind::Module(name) = import.kind {
            self.names
                .entry((import.module, name))
                .or_default()
                .push(self.values.len());
        }
        if matches!(import.kind, UseKind::Glob) {
            self.globs
                .entry(import.module)
                .or_default()
                .push(self.values.len());
        }
        self.values.push(import);
    }

    pub(crate) fn globs(&self, module: ModuleId) -> impl Iterator<Item = (usize, &Use)> {
        self.globs
            .get(&module)
            .into_iter()
            .flatten()
            .map(|&index| (index, &self.values[index]))
    }

    /// The most recent matching binding not already followed by this lookup.
    pub(crate) fn named(
        &self,
        module: ModuleId,
        name: Symbol,
        followed: &[usize],
    ) -> Option<(usize, &Use)> {
        let &index = self
            .names
            .get(&(module, name))?
            .iter()
            .rev()
            .find(|index| !followed.contains(index))?;
        Some((index, &self.values[index]))
    }
}

impl Extend<Use> for Uses {
    fn extend<T: IntoIterator<Item = Use>>(&mut self, imports: T) {
        for import in imports {
            self.push(import);
        }
    }
}

impl FromIterator<Use> for Uses {
    fn from_iter<T: IntoIterator<Item = Use>>(imports: T) -> Self {
        let mut uses = Self::new();
        uses.extend(imports);
        uses
    }
}

impl Deref for Uses {
    type Target = [Use];

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FileId, Path, Visibility};
    use flux_syntax::TextRange;

    fn import(module: ModuleId, kind: UseKind) -> Use {
        Use {
            module,
            file: FileId::UNKNOWN,
            vis: Visibility::Module(module),
            path: Path::single(Symbol::intern("source")),
            kind,
            range: TextRange::default(),
        }
    }

    #[test]
    fn indexed_lookup_matches_reverse_scan_after_every_append() {
        let a = ModuleId::from_raw(0);
        let b = ModuleId::from_raw(1);
        let x = Symbol::intern("x");
        let y = Symbol::intern("y");
        let missing = Symbol::intern("missing");
        let mut uses = Uses::default();
        for value in [
            import(a, UseKind::Named(x)),
            import(b, UseKind::Named(x)),
            import(a, UseKind::Module(y)),
            import(a, UseKind::Glob),
            import(a, UseKind::Named(x)),
            import(a, UseKind::Anonymous { module_only: false }),
            import(a, UseKind::Module(x)),
        ] {
            uses.push(value);
            for mask in 0..1usize << uses.len() {
                let followed: Vec<_> = (0..uses.len()).filter(|i| mask & (1 << i) != 0).collect();
                for module in [a, b] {
                    for name in [x, y, missing] {
                        let expected = uses.iter().enumerate().rev().find(|(index, u)| {
                            !followed.contains(index)
                                && u.module == module
                                && matches!(u.kind, UseKind::Named(alias) | UseKind::Module(alias) if alias == name)
                        });
                        assert_eq!(
                            uses.named(module, name, &followed).map(|(index, _)| index),
                            expected.map(|(index, _)| index),
                        );
                    }
                }
            }
        }
        let mut cloned = uses.clone();
        cloned.extend([import(b, UseKind::Module(y)), import(a, UseKind::Named(x))]);
        assert_eq!(cloned.named(a, x, &[]).unwrap().0, 8);
        assert_eq!(uses.named(a, x, &[]).unwrap().0, 6);
        assert!(uses.named(b, y, &[]).is_none());
        assert_eq!(cloned.named(b, y, &[]).unwrap().0, 7);
    }

    #[test]
    fn archive_loading_rebuilds_the_name_index() {
        use crate::flib::{CrateIndices, Decoder, Encoder, IdMap, read_items, write_items};
        use crate::{CrateId, ItemTree, SourceDb};

        let mut db = SourceDb::new();
        let root = db.add("dep.rs", "pub fn value() {} pub use value as renamed;");
        let original = ItemTree::new(&mut db, root, Symbol::intern("dep"), false);
        assert!(
            original.diagnostics.is_empty(),
            "{:?}",
            original.diagnostics
        );
        let mut ids = IdMap::strict();
        CrateIndices::collect(&original, CrateId(0))
            .add_mapping(&mut ids, CrateId(0), 0)
            .unwrap();
        let mut encoder = Encoder::with_ids(ids);
        write_items(&original, CrateId(0), &mut encoder).unwrap();
        let bytes = encoder.into_bytes();
        let mut decoded = ItemTree::default();
        let mut decoder = Decoder::with_ids(&bytes, IdMap::strict());
        read_items(&mut decoded, &mut db, &mut decoder).unwrap();
        decoder.finish().unwrap();
        let name = Symbol::intern("renamed");
        let binding = &decoded.uses[0];
        assert_eq!(decoded.uses.named(binding.module, name, &[]).unwrap().0, 0);
        assert!(decoded.uses.named(binding.module, name, &[0]).is_none());
    }
}
