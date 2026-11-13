//! Matching caller evidence without turning normalization into new evidence.

use super::{Predicate, Ty, TyKind};

/// Declaration inputs preserve written parameter arguments; rigid declaration
/// proofs preserve the entire membership. Value proofs may compare normalized
/// forms, but must still start from an independently supplied membership.
#[derive(Clone, Copy)]
pub(super) enum BoundMatching {
    Original,
    DeclarationInput,
    Normalized,
}

pub(super) fn map_predicate(p: Predicate, mut normalize: impl FnMut(Ty) -> Ty) -> Predicate {
    Predicate {
        self_ty: normalize(p.self_ty),
        trait_ref: super::TraitRef {
            args: crate::ty::map_list(p.trait_ref.args, normalize),
            ..p.trait_ref
        },
    }
}

/// Compare both sides under the same normalization rules. The closure chooses
/// which equalities/impls are available; this function never selects an impl.
pub(super) fn matches(
    memberships: &[Predicate],
    goal: Predicate,
    mode: BoundMatching,
    mut normalize: impl FnMut(Predicate) -> Predicate,
) -> bool {
    if memberships.contains(&goal) {
        return true;
    }
    if matches!(mode, BoundMatching::Original) {
        return false;
    }
    let mut normalized_goal = None;
    memberships.iter().copied().any(|fact| {
        fact.trait_ref.trait_ == goal.trait_ref.trait_
            && (!matches!(mode, BoundMatching::DeclarationInput)
                || !matches!(fact.self_ty.kind(), TyKind::Param(_))
                || fact.trait_ref.args == goal.trait_ref.args)
            && normalize(fact) == *normalized_goal.get_or_insert_with(|| normalize(goal))
    })
}
