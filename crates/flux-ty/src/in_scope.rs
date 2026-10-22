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
        let def_map = &items.def_map;
        let named = |module: ModuleId| {
            def_map.scopes[module.index()]
                .types
                .values()
                .filter_map(|binding| match binding.def {
                    Def::Trait(t) => Some(t),
                    _ => None,
                })
        };
        let prelude: Vec<TraitId> = def_map.prelude.into_iter().flat_map(named).collect();
        let start = |traits: &Vec<TraitId>| {
            u32::try_from(traits.len()).expect("fewer than 2^32 traits in scope")
        };
        let mut traits = Vec::new();
        let mut starts = Vec::with_capacity(def_map.scopes.len() + 1);
        let mut module_traits = Vec::new();
        for index in 0..def_map.scopes.len() {
            starts.push(start(&traits));
            // A block's scope, those of the blocks around it, its module's.
            let mut module = Some(ModuleId::from_raw(index as u32));
            while let Some(m) = module {
                module_traits.extend(named(m));
                module = items.modules[m]
                    .parent
                    .filter(|_| items.modules[m].block.is_some());
            }
            module_traits.extend(&prelude);
            module_traits.sort_unstable_by_key(|t| t.index());
            module_traits.dedup();
            traits.append(&mut module_traits);
        }
        starts.push(start(&traits));
        TraitsInScope { traits, starts }
    }

    /// The traits in scope in `module`, by index.
    pub fn get(&self, module: ModuleId) -> &[TraitId] {
        match self.starts.get(module.index()..module.index() + 2) {
            Some(&[start, end]) => &self.traits[start as usize..end as usize],
            _ => &[],
        }
    }
}
