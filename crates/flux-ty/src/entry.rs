//! Trusted standard-library definitions used by the executable entry point.
use flux_hir::{FunctionId, ItemTree, ModuleId, TraitId};

fn termination_module(items: &ItemTree, module: ModuleId) -> bool {
    items.is_std(module)
        && items.modules[module].name.as_str() == "termination"
        && items.modules[module].parent.is_some_and(|parent| {
            items.modules[parent].name.as_str() == "process"
                && items.modules[parent].parent
                    == Some(items.module_of(items.modules[module].krate))
        })
}

/// The actual std termination trait, excluding user crates with matching names.
pub fn termination_trait(items: &ItemTree) -> Option<TraitId> {
    items.traits.iter().find_map(|(id, data)| {
        (data.name.as_str() == "Termination" && termination_module(items, data.loc.module))
            .then_some(id)
    })
}

/// The ordinary generic std adapter responsible for reporting a main result.
pub fn main_runner(items: &ItemTree, sigs: &crate::Signatures) -> Option<FunctionId> {
    let root = items.module_of(items.main_crate());
    let (id, _) = items.functions.iter().find(|(_, f)| {
        f.name.as_str() == items.entry_function_name(items.main_crate())
            && f.loc.module == root
            && f.owner.is_none()
    })?;
    if [crate::Ty::UNIT, crate::Ty::NEVER, crate::Ty::ERROR].contains(&sigs[id].ret) {
        return None;
    }

    items.functions.iter().find_map(|(id, data)| {
        (data.name.as_str() == "__flux_run_main" && termination_module(items, data.loc.module))
            .then_some(id)
    })
}
