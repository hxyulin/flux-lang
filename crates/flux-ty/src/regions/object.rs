//! Region rows for associated equalities inherited by trait objects.
use flux_hir::{Generics, ItemTree, Owner};

use super::{Elided, Region, Walker, instantiate};
use crate::{Projection, TraitRef, Ty, TypeScope};

pub(super) type ProjectionInputs = (Vec<Region>, Vec<Box<[Region]>>);
type RegionGraph = (Vec<Principal>, Vec<(Projection, Box<[Region]>)>);

#[derive(Clone, PartialEq, Eq)]
struct Principal {
    trait_ref: TraitRef,
    lifetimes: Vec<Region>,
    arguments: Vec<Box<[Region]>>,
}

pub(super) fn object_equalities(
    items: &ItemTree,
    principal: TraitRef,
    lifetimes: &[Region],
    arguments: &[Box<[Region]>],
) -> Option<Vec<(Projection, Box<[Region]>)>> {
    region_graph(
        items,
        Ty::param(u32::MAX, flux_intern::kw::SelfUpper),
        principal,
        lifetimes,
        arguments,
    )
    .map(|(_, equalities)| equalities)
}

pub(super) fn inherited_inputs(
    items: &ItemTree,
    self_ty: Ty,
    principal: TraitRef,
    lifetimes: &[Region],
    arguments: &[Box<[Region]>],
    target: TraitRef,
) -> Option<ProjectionInputs> {
    let (principals, _) = region_graph(items, self_ty, principal, lifetimes, arguments)?;
    let principal = principals
        .into_iter()
        .find(|principal| principal.trait_ref == target)?;
    Some((principal.lifetimes, principal.arguments))
}

fn region_graph(
    items: &ItemTree,
    self_ty: Ty,
    principal: TraitRef,
    lifetimes: &[Region],
    arguments: &[Box<[Region]>],
) -> Option<RegionGraph> {
    let mut pending = vec![Principal {
        trait_ref: principal,
        lifetimes: lifetimes.to_vec(),
        arguments: arguments.to_vec(),
    }];
    let mut visited = std::collections::HashMap::new();
    let mut result = Vec::new();
    let mut principals = Vec::new();
    while let Some(current) = pending.pop() {
        if let Some(previous) = visited.insert(current.trait_ref.trait_, current.clone()) {
            if previous != current {
                return None;
            }
            continue;
        }
        principals.push(current.clone());
        let data = &items.traits[current.trait_ref.trait_];
        if current.lifetimes.len() != data.generics.lifetimes.len()
            || current.arguments.len() != data.generics.params.len()
        {
            return None;
        }
        let names = crate::generic_names(
            items,
            Some(Owner::Trait(current.trait_ref.trait_)),
            &Generics::default(),
        );
        let scope = TypeScope {
            self_ty: Some(Ty::param(0, flux_intern::kw::SelfUpper)),
            generics: &names,
            ..TypeScope::default()
        };
        let actual: Vec<_> = std::iter::once(self_ty)
            .chain(current.trait_ref.args.iter().copied())
            .collect();
        let instantiate_row = |ty, row: &[Region]| -> Option<Box<[Region]>> {
            if row
                .iter()
                .any(|region| matches!(region, Region::Late(_) | Region::Erased))
            {
                return None;
            }
            let mut map = |region| match region {
                Region::Param(index) => current
                    .lifetimes
                    .get(index as usize)
                    .copied()
                    .unwrap_or(Region::Erased),
                region => region,
            };
            let args = |index: u32| {
                index
                    .checked_sub(1)
                    .and_then(|index| current.arguments.get(index as usize))
                    .map_or(Vec::new(), |row| row.to_vec())
            };
            let mut out = Vec::new();
            instantiate(items, ty, row, &mut map, &args, &mut out);
            Some(out.into())
        };
        let bounds = data.supertraits.iter().chain(data.generics.predicates.iter().filter(|predicate| {
            matches!(&predicate.ty, flux_hir::TypeRef::Path(path) if path.as_name() == Some(flux_intern::kw::SelfUpper))
        }).map(|predicate| &predicate.bound));
        for bound in bounds {
            if !bound.bound_lifetimes.is_empty() {
                return None;
            }
            let mut errors = Vec::new();
            let (edge, projections) = crate::traits::resolve_bound(
                items,
                data.loc.module,
                bound,
                Ty::param(0, flux_intern::kw::SelfUpper),
                scope,
                &mut errors,
            )?;
            if !errors.is_empty() {
                return None;
            }
            let mut walker = Walker::new(
                items,
                data.loc.module,
                data.generics
                    .lifetimes
                    .iter()
                    .map(|lifetime| Some(lifetime.name))
                    .collect(),
            );
            walker.scope = scope;
            walker.elided = Elided::NotAllowed;
            let written = bound.path.args(bound.path.segments.len().saturating_sub(1));
            for projection in projections {
                let binding = written?
                    .bindings
                    .iter()
                    .find(|binding| binding.name == projection.alias.name)?;
                let row = walker.regions(&binding.ty, projection.ty);
                result.push((
                    projection.subst(&actual),
                    instantiate_row(projection.ty, &row)?,
                ));
            }
            let mut own = Vec::new();
            walker.trait_lifetimes(&bound.path, edge.trait_, false, &mut own);
            let own = own
                .into_iter()
                .map(|region| match region {
                    Region::Param(index) => current.lifetimes.get(index as usize).copied(),
                    Region::Static => Some(Region::Static),
                    _ => None,
                })
                .collect::<Option<Vec<_>>>()?;
            let mut rows = Vec::new();
            for (index, &ty) in edge.args.iter().enumerate() {
                let written = written?.types.get(index)?;
                let row = walker.regions(written, ty);
                rows.push(instantiate_row(ty, &row)?);
            }
            if !walker.errors.is_empty() {
                return None;
            }
            pending.push(Principal {
                trait_ref: TraitRef {
                    args: crate::ty::map_list(edge.args, |ty| ty.subst(&actual)),
                    ..edge
                },
                lifetimes: own,
                arguments: rows,
            });
        }
    }
    Some((principals, result))
}
