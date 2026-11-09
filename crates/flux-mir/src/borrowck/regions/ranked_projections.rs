//! Instantiate associated equalities from their original quantified inputs.
use super::*;

#[derive(Clone)]
pub(super) struct RankedProjection {
    pub alias: AliasTy,
    input: ProjectionInputRecipe,
    output: Ty,
    row: Box<[Region]>,
}

fn extend(
    items: &ItemTree,
    generics: &flux_hir::Generics,
    predicates: &[flux_ty::Predicate],
    inputs: (&[Box<[Region]>], &[TraitRefRegions]),
    projections: &[flux_ty::Projection],
    outputs: &[Box<[Region]>],
    out: &mut Vec<RankedProjection>,
) {
    let (subjects, traits) = inputs;
    let mut next = 0;
    for (index, written) in generics.predicates.iter().enumerate() {
        let args = written
            .bound
            .path
            .args(written.bound.path.segments.len().saturating_sub(1));
        for binding in args.into_iter().flat_map(|a| &a.bindings) {
            if let (Some(&predicate), Some(own), Some(rows), Some(projection), Some(output)) = (
                predicates.get(index),
                subjects.get(index),
                traits.get(index),
                projections.get(next),
                outputs.get(next),
            ) && rows.bound != 0
                && flux_ty::fn_trait_kind(items, predicate.trait_ref.trait_).is_none()
                && binding.name == projection.alias.name
            {
                out.push(RankedProjection {
                    alias: projection.alias,
                    input: (predicate, own.clone(), rows.clone()),
                    output: projection.ty,
                    row: output.clone(),
                });
            }
            next += 1;
        }
    }
}

pub(super) fn for_function(
    items: &ItemTree,
    sigs: &Signatures,
    function: flux_hir::FunctionId,
) -> Vec<RankedProjection> {
    let mut out = Vec::new();
    if let Some(flux_hir::Owner::Impl(imp)) = items.functions[function].owner
        && let Some(r) = sigs.regions.impls.get(imp)
    {
        extend(
            items,
            &items.impls[imp].generics,
            sigs.impl_predicates.get(imp).map_or(&[], |p| &p[..]),
            (&r.bound_selfs, &r.bound_traits),
            sigs.impl_projections.get(imp).map_or(&[], |p| &p[..]),
            &r.projections,
            &mut out,
        );
    }
    if let Some(flux_hir::Owner::Trait(owner)) = items.functions[function].owner
        && let Some(full) = flux_ty::regions::trait_bound_schemes(items, sigs, owner)
    {
        extend(
            items,
            &full.schema,
            &full.predicates,
            (&full.subjects, &full.traits),
            &sigs.traits[owner].header_projections,
            &full.projections,
            &mut out,
        );
    }
    if let Some(r) = sigs.regions.fns.get(function) {
        extend(
            items,
            &items.functions[function].generics,
            sigs.fn_predicates.get(function).map_or(&[], |p| &p[..]),
            (&r.bound_selfs, &r.bound_traits),
            sigs.fn_projections.get(function).map_or(&[], |p| &p[..]),
            &r.projections,
            &mut out,
        );
    }
    out
}

impl Cx<'_> {
    pub(super) fn ranked_projection_output(
        &mut self,
        scheme: &RankedProjection,
        owner: &[Var],
        inputs: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        let (_, subject, traits) = &scheme.input;
        if owner.len() != traits.lifetimes.len() || inputs.len() != traits.args.len() + 1 {
            return None;
        }
        let mut bound = vec![None; traits.bound as usize];
        for (recipe, actual) in std::iter::once((traits.lifetimes.as_ref(), owner))
            .chain(std::iter::once((subject.as_ref(), inputs[0].as_slice())))
            .chain(
                traits
                    .args
                    .iter()
                    .zip(&inputs[1..])
                    .map(|(a, b)| (a.as_ref(), b.as_slice())),
            )
        {
            if recipe.len() != actual.len() {
                return None;
            }
            for (&region, &var) in recipe.iter().zip(actual) {
                let expected = match region {
                    Region::Static => Some(STATIC),
                    Region::Param(index) if self.universal_of.contains_key(&(1 + index)) => {
                        Some(1 + index)
                    }
                    Region::Late(index) => {
                        let previous = bound.get_mut(index as usize)?;
                        Some(*previous.get_or_insert(var))
                    }
                    Region::Erased => None,
                    Region::Param(_) => return None,
                };
                if let Some(expected) = expected {
                    self.equate(expected, var, Category::CallArgument, self.call_range);
                }
            }
        }
        let mut map = |region| match region {
            Region::Static => Some(STATIC),
            Region::Param(index) => Some(1 + index),
            Region::Late(index) => bound.get(index as usize).copied().flatten(),
            Region::Erased => None,
        };
        // A tautological equality keeps its written occurrence row rather than
        // recursively trying to select that same abstract output again.
        if scheme.output == Ty::alias(scheme.alias)
            && self.normalize(scheme.output) == scheme.output
        {
            return scheme.row.iter().copied().map(&mut map).collect();
        }
        self.instantiate_sig(scheme.output, &scheme.row, &mut map, &[], &[])
    }
}
