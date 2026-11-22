//! The traits in scope in each module, whose methods its bodies may
//! call: listed once per module rather than for every body.

use flux_hir::def_map::Def;
use flux_hir::{ItemTree, ModuleId, TraitId};

/// The traits in scope in each module, by index, without duplicates:
/// those its scope names (with a block's, those of the blocks around it
/// and its module's) and the prelude's. One flat array, each
/// module's from its start.
#[derive(Debug, Clone, Default)]
pub struct TraitsInScope {
    traits: Vec<TraitId>,
    /// Where each module's traits start in `traits`, and where the last
    /// module's end.
    starts: Vec<u32>,
}

impl TraitsInScope {
    /// The traits in scope in each module of `items`.
    pub fn new(items: &ItemTree) -> TraitsInScope {
        let mut scopes = Self::default();
        scopes.append(items);
        scopes
    }

    /// Updates the table when an import appends modules to the item tree.
    pub(crate) fn append(&mut self, items: &ItemTree) {
        let def_map = &items.def_map;
        let named = |module: ModuleId| {
            def_map.scopes[module.index()]
                .types
                .values()
                .chain(&def_map.scopes[module.index()].anonymous_traits)
                .filter_map(|binding| match binding.def {
                    Def::Trait(t) => Some(t),
                    _ => None,
                })
        };
        let start = |traits: &[TraitId]| {
            u32::try_from(traits.len()).expect("fewer than 2^32 traits in scope")
        };
        self.starts.pop();
        self.starts
            .reserve(def_map.scopes.len() + 1 - self.starts.len());
        let mut module_traits = Vec::new();
        for index in self.starts.len()..def_map.scopes.len() {
            self.starts.push(start(&self.traits));
            // A block's scope, those of the blocks around it, its module's.
            let mut module = Some(ModuleId::from_raw(index as u32));
            while let Some(m) = module {
                module_traits.extend(named(m));
                module = items.modules[m]
                    .parent
                    .filter(|_| items.modules[m].block.is_some());
            }
            let own = ModuleId::from_raw(index as u32);
            if let Some(prelude) = items.prelude_for(own) {
                module_traits.extend(named(prelude));
            }
            module_traits.sort_unstable_by_key(|t| t.index());
            module_traits.dedup();
            self.traits.append(&mut module_traits);
        }
        self.starts.push(start(&self.traits));
    }

    /// The traits in scope in `module`, by index.
    pub fn get(&self, module: ModuleId) -> &[TraitId] {
        match self.starts.get(module.index()..module.index() + 2) {
            Some(&[start, end]) => &self.traits[start as usize..end as usize],
            _ => &[],
        }
    }
}
