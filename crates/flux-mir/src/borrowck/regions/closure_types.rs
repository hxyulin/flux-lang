//! Abstract type-outlives obligations exported by a closure body.
use super::*;

impl Cx<'_> {
    /// Abstract components keep the defining function's generic namespace.
    /// The closure value supplies its actual parent type/const arguments.
    pub(super) fn closure_type_requirements(
        &mut self,
        required: &ClosureRequirements,
        args: flux_ty::GenericArgs,
        defer_local_targets: bool,
        mut target: impl FnMut(Ext) -> Option<Var>,
        range: TextRange,
    ) {
        if required.type_outlives.is_empty() {
            return;
        }
        if !self.closure_parent_regions(required, range) {
            return;
        }
        let parent = flux_ty::ClosureArgs::split(args).parent;
        for &(ty, external, _) in &required.type_outlives {
            let instance = ty.subst(parent);
            let Some(target) = target(external) else {
                // Construction has no invocation rows. A subsequent callable
                // proof/output or direct call must instantiate these targets.
                if defer_local_targets
                    && matches!(external, Ext::Arg(_) | Ext::Ret(_) | Ext::SelfRef)
                {
                    continue;
                }
                self.errors.push(Diagnostic::new(
                    range,
                    "closure type-outlives target is not represented",
                ));
                continue;
            };
            // A bare type has no lifetime occurrence rows. Never invent those
            // rows if substitution reveals references in an abstract component.
            if instance.has_error() || self.slots(self.normalize(instance)) != 0 {
                self.errors.push(Diagnostic::new(
                    range,
                    "closure type-outlives substitution needs represented lifetime rows",
                ));
                continue;
            }
            self.type_outlives(instance, target, range);
        }
    }
}
