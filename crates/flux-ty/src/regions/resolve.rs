//! Resolving the lifetimes of every signature, and the checks on them.

use super::*;

type Errors = Vec<(flux_hir::FileId, TextRange, String)>;

/// A bound on a trait's abstract Self is inherited through its supertraits.
/// Follow only lifetimeless owners: the erased TraitRef does not represent
/// lifetime arguments on other supertrait edges. Static Self needs no such
/// substitution, and never follows bounds on a trait's other type arguments.
fn inherit_static_self(items: &ItemTree, sigs: &mut Signatures, from: flux_hir::ItemCounts) {
    let self_static = |bounds: &[Outlives<Region>]| {
        bounds.iter().any(|bound| {
            matches!(*bound, Outlives::Type(ty, Region::Static)
                if matches!(ty.kind(), TyKind::Param(p) if p.index == 0))
        })
    };
    let mut children = vec![Vec::new(); items.traits.len()];
    let mut requires_static = vec![false; items.traits.len()];
    let mut pending = Vec::new();
    for (id, data) in items.traits.iter() {
        if !data.generics.lifetimes.is_empty() {
            continue;
        }
        if sigs
            .regions
            .traits
            .get(id)
            .is_some_and(|r| self_static(&r.outlives))
        {
            requires_static[id.index()] = true;
            pending.push(id);
        }
        if let Some(signature) = sigs.traits.get(id) {
            for parent in &signature.supertraits {
                if items.traits[parent.trait_].generics.lifetimes.is_empty() {
                    children[parent.trait_.index()].push(id);
                }
            }
        }
    }
    // Reverse edges let each static contract flow to its children once. An
    // unrelated chain needs only the graph construction, not ancestor walks.
    while let Some(parent) = pending.pop() {
        for &child in &children[parent.index()] {
            if !requires_static[child.index()] {
                requires_static[child.index()] = true;
                pending.push(child);
            }
        }
    }
    for (id, _) in items.traits.iter().skip(from.traits as usize) {
        if requires_static[id.index()]
            && let Some(regions) = sigs.regions.traits.get_mut(id)
            && !self_static(&regions.outlives)
        {
            let self_ty = flux_solve::traits::trait_self_predicate(items, id).self_ty;
            let mut outlives = regions.outlives.to_vec();
            outlives.push(Outlives::Type(self_ty, Region::Static));
            regions.outlives = outlives.into();
        }
    }
}

/// Family-local binders are appended after owner lifetimes. The where clause
/// is a prerequisite at each projection occurrence, never a new caller assumption.
#[allow(clippy::too_many_arguments)]
fn family_contract(
    items: &ItemTree,
    sigs: &Signatures,
    module: ModuleId,
    owner: flux_hir::Owner,
    owner_names: &[LifetimeName],
    generics: &Generics,
    self_ty: Ty,
    self_regions: Option<&[Region]>,
    value: Option<(&TypeRef, Ty)>,
    output_bounds: &[LifetimeRef],
    output_traits: &[(&flux_hir::Bound, crate::TraitRef)],
    output_projections: &[(
        &flux_hir::AssocBinding,
        Option<&flux_hir::GenericArgs>,
        crate::Projection,
    )],
    schema: Option<&crate::traits::AssociatedTypeSig>,
    errors: &mut Vec<(TextRange, String)>,
) -> FamilyRegions {
    for lifetime in &generics.lifetimes {
        if owner_names
            .iter()
            .any(|owner| owner.name == Some(lifetime.name))
        {
            errors.push((
                lifetime.range,
                "lifetime name shadows a lifetime already in scope".into(),
            ));
        }
    }
    let mut names = owner_names.to_vec();
    names.extend(own_names(generics));
    let type_names = crate::generic_names(items, Some(owner), generics);
    let constants = crate::generic_consts(items, Some(owner), generics);
    let mut predicates = flux_solve::traits::owner_predicates(items, sigs, Some(owner));
    if let Some(schema) = schema {
        predicates.extend(schema.predicates.iter().copied());
    }
    let predicates = flux_solve::traits::elaborate(sigs, &predicates);
    let mut walker = Walker::new(items, module, names.iter().map(|n| n.name).collect());
    walker.scope = TypeScope {
        self_ty: Some(self_ty),
        bounds: &predicates,
        generics: &type_names,
        consts: &constants,
        signature_of: Some("associated types"),
        ..TypeScope::default()
    };
    walker.self_regions = self_regions;
    if let Some(generics) = items.owner_generics(Some(owner)) {
        walker.add_projection_bounds(generics);
    }
    walker.add_projection_bounds(generics);
    walker.self_projection_owner = match owner {
        flux_hir::Owner::Trait(id) => Some((
            id,
            (0..owner_names.len() as u32).map(Region::Param).collect(),
        )),
        flux_hir::Owner::Impl(id) => sigs.impl_traits.get(id).and_then(|tr| {
            sigs.regions
                .impls
                .get(id)
                .map(|r| (tr.trait_, r.trait_lifetimes.clone()))
        }),
    };
    walker.elided = Elided::Missing(None);
    let mut outlives = Vec::new();
    for predicate in &generics.outlives {
        let a = walker.named(predicate.longer);
        let b = walker.named(predicate.shorter);
        if a != Region::Erased && b != Region::Erased {
            outlives.push(Outlives::Region(a, b));
        }
    }
    for predicate in &generics.type_outlives {
        let r = walker.named(predicate.lifetime);
        let ty = crate::resolve_type(
            items,
            module,
            &predicate.ty,
            walker.scope,
            predicate.ty_range,
            errors,
        );
        if has_projection_outlives_inputs(items, ty) {
            errors.push((
                predicate.ty_range,
                "associated family projection outlives subjects are not supported yet".into(),
            ));
            continue;
        }
        let row = walker.regions(&predicate.ty, ty);
        if r != Region::Erased {
            for a in stored_regions(items, ty, &row) {
                if a != Region::Erased && a != r {
                    outlives.push(Outlives::Region(a, r));
                }
            }
            outlives_params(sigs, ty, r, &mut outlives);
        }
    }
    let output_outlives = output_bounds
        .iter()
        .map(|&l| walker.named(l))
        .filter(|r| *r != Region::Erased)
        .collect();
    let output_traits = output_traits
        .iter()
        .map(|(written, bound)| {
            let mut lifetimes = Vec::new();
            walker.trait_lifetimes(&written.path, bound.trait_, false, &mut lifetimes);
            let args = written
                .path
                .args(written.path.segments.len().saturating_sub(1));
            let rows = bound
                .args
                .iter()
                .enumerate()
                .map(|(i, &ty)| {
                    args.and_then(|a| a.types.get(i)).map_or_else(
                        || vec![Region::Erased; slots(items, ty)].into_boxed_slice(),
                        |written| walker.regions(written, ty),
                    )
                })
                .collect();
            (
                *bound,
                TraitRefRegions {
                    bound: 0,
                    lifetimes: lifetimes.into(),
                    args: rows,
                },
            )
        })
        .collect();
    let output_projections = output_projections
        .iter()
        .map(|(written, args, projection)| {
            (
                *projection,
                projection_regions(&mut walker, written, *args, projection),
            )
        })
        .collect();
    walker.selections = Some(Vec::new());
    let row = value.map_or_else(Box::default, |(written, ty)| walker.regions(written, ty));
    let selections = walker.selections.take().unwrap_or_default().into();
    errors.append(&mut walker.errors);
    let (bound_selfs, bound_traits, projections) = schema.map_or_else(
        || (Box::default(), Box::default(), Box::default()),
        |schema| {
            bound_regions(
                items,
                module,
                generics,
                &names.iter().map(|n| n.name).collect::<Vec<_>>(),
                &type_names,
                &schema.predicates,
                &schema.projections,
                errors,
            )
        },
    );
    dedup(&mut outlives);
    FamilyRegions {
        names: names.into(),
        owner: owner_names.len() as u32,
        value: row,
        outlives: outlives.into(),
        output_outlives,
        output_traits,
        output_projections,
        bound_selfs,
        bound_traits,
        projections,
        selections,
    }
}

/// Reconstruct an ordinary associated output without using its saved region rows.
pub(super) fn ordinary_associated_contract(
    items: &ItemTree,
    sigs: &Signatures,
    owner: flux_hir::TraitId,
    name: Symbol,
) -> Option<(FamilyRegions, Vec<crate::TraitRef>, Vec<crate::Projection>)> {
    let data = &items.traits[owner];
    let family = data.types.iter().find(|f| f.name == name)?;
    let schema = sigs
        .traits
        .get(owner)?
        .families
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, schema)| schema)?;
    if schema.owner_params != 1 + data.generics.params.len() as u32
        || schema.lifetimes != 0
        || !schema.params.is_empty()
        || !schema.predicates.is_empty()
        || !schema.projections.is_empty()
    {
        return None;
    }

    if !family.generics.lifetimes.is_empty()
        || !family.generics.params.is_empty()
        || !family.generics.predicates.is_empty()
        || !family.generics.outlives.is_empty()
        || !family.generics.type_outlives.is_empty()
    {
        return None;
    }
    let owner_names: Vec<_> = own_names(&data.generics).collect();
    let names = crate::generic_names(items, Some(flux_hir::Owner::Trait(owner)), &family.generics);
    let self_ty = Ty::param(0, kw::SelfUpper);
    let predicate = flux_solve::traits::trait_self_predicate(items, owner);
    let bounds =
        flux_solve::traits::owner_predicates(items, sigs, Some(flux_hir::Owner::Trait(owner)));
    let scope = TypeScope {
        self_ty: Some(self_ty),
        generics: &names,
        bounds: &bounds,
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        ..TypeScope::default()
    };
    let alias = Ty::alias(flux_solve::traits::alias_of(
        items,
        predicate.trait_ref,
        self_ty,
        name,
    ));
    let mut errors = Vec::new();
    let mut traits = Vec::new();
    let mut projections = Vec::new();
    for bound in &family.bounds {
        let (target, implied) =
            crate::traits::resolve_bound(items, data.loc.module, bound, alias, scope, &mut errors)?;
        let args = bound.path.args(bound.path.segments.len().saturating_sub(1));
        let bindings = args.map_or(&[][..], |a| &a.bindings);
        if bindings.len() != implied.len() {
            return None;
        }
        traits.push((bound, target));
        projections.extend(
            bindings
                .iter()
                .zip(implied)
                .map(|(binding, projection)| (binding, args, projection)),
        );
    }
    let contract = family_contract(
        items,
        sigs,
        data.loc.module,
        flux_hir::Owner::Trait(owner),
        &owner_names,
        &family.generics,
        self_ty,
        None,
        None,
        &family.lifetime_bounds,
        &traits,
        &projections,
        None,
        &mut errors,
    );
    errors.is_empty().then(|| {
        (
            contract,
            traits.into_iter().map(|(_, t)| t).collect(),
            projections.into_iter().map(|(_, _, p)| p).collect(),
        )
    })
}

fn resolve_trait_families(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    for (id, data) in items.traits.iter().skip(from.traits as usize) {
        let owner_names = sigs.regions.traits.get(id).map_or(&[][..], |r| &r.names);
        let mut list = Vec::new();
        let families = data.types.iter().map(|family| {
            let schema=sigs.traits.get(id).and_then(|t|t.families.iter().find(|(n,_)|*n==family.name)).map(|(_,s)|s);
            let resolved:Vec<_>=sigs.traits.get(id).into_iter().flat_map(|t|t.type_bounds.iter()).filter(|(name,_)|*name==family.name).map(|(_,bound)|*bound).collect();
            let output:Vec<_>=family.bounds.iter().zip(resolved).collect();
            let implied:Vec<_>=sigs.traits.get(id).into_iter().flat_map(|t|t.implied.iter()).filter(|p|matches!(p.alias.args.first().map(|t|t.kind()),Some(TyKind::Alias(alias))if alias.trait_==id && alias.name==family.name)).copied().collect();
            let bindings=family.bounds.iter().flat_map(|b| {let args=b.path.args(b.path.segments.len().saturating_sub(1));args.into_iter().flat_map(move |args|args.bindings.iter().map(move |binding|(binding,Some(args))))});
            let projections:Vec<_>=bindings.zip(implied).map(|((binding,args),projection)|(binding,args,projection)).collect();
            let mut contract = family_contract(items,sigs,data.loc.module,flux_hir::Owner::Trait(id),owner_names,&family.generics,Ty::param(0,kw::SelfUpper),None,None,&family.lifetime_bounds,&output,&projections,schema,&mut list);
            // Generic associated outputs are Sized unless explicitly relaxed.
            if let Some(sized) = sigs.builtins.sized
                && !family.maybe_unsized
                && (!family.generics.lifetimes.is_empty() || !family.generics.params.is_empty())
            {
                let mut bounds = contract.output_traits.into_vec();
                let bound = crate::TraitRef { trait_: sized, name: items.traits[sized].name, args: &[] };
                if !bounds.iter().any(|(existing, _)| *existing == bound) {
                    bounds.push((bound, TraitRefRegions::default()));
                }
                contract.output_traits = bounds.into();
            }
            (family.name, contract)
        }).collect();
        sigs.regions.traits.get_mut(id).unwrap().families = families;
        errors.extend(list.into_iter().map(|(r, m)| (data.loc.file, r, m)));
    }
}

fn resolve_impl_families(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    for (id, data) in items.impls.iter().skip(from.impls as usize) {
        let Some(owner) = sigs.regions.impls.get(id) else {
            continue;
        };
        let mut list = Vec::new();
        let families = data
            .types
            .iter()
            .map(|family| {
                let ty = flux_solve::traits::impl_type(sigs, id, family.name).unwrap_or(Ty::ERROR);
                let schema = sigs
                    .impl_families
                    .get(id)
                    .and_then(|f| f.iter().find(|(n, _)| *n == family.name))
                    .map(|(_, s)| s);
                (
                    family.name,
                    family_contract(
                        items,
                        sigs,
                        data.module,
                        flux_hir::Owner::Impl(id),
                        &owner.names,
                        &family.generics,
                        sigs.impls[id],
                        Some(&owner.self_ty),
                        Some((&family.ty, ty)),
                        &[],
                        &[],
                        &[],
                        schema,
                        &mut list,
                    ),
                )
            })
            .collect();
        sigs.regions.impls.get_mut(id).unwrap().families = families;
        errors.extend(list.into_iter().map(|(r, m)| (data.file, r, m)));
    }
}

/// Merely writing a family equality must satisfy the family's prerequisites,
/// even in an unused declaration whose RHS has no reference slots.
fn check_family_equality_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    let mut check = |file,
                     range,
                     projections: &[crate::Projection],
                     assumptions: &[Outlives<Region>]| {
        for projection in projections
            .iter()
            .filter(|p| !p.alias.family_args.is_empty())
        {
            let alias = projection.alias;
            let Some((_, family)) = sigs
                .regions
                .traits
                .get(alias.trait_)
                .and_then(|t| t.families.iter().find(|(name, _)| *name == alias.name))
            else {
                continue;
            };
            let args = alias.template_args();
            for requirement in &family.outlives {
                let valid = match *requirement {
                    Outlives::Region(a, b) => outlives_known(Outlives::Region(a, b), assumptions),
                    Outlives::Type(ty, b) => {
                        let ty = flux_solve::traits::normalize(items, sigs, &[], ty.subst(&args));
                        outlives_type_components(sigs, ty)
                            .into_iter()
                            .all(|component| {
                                outlives_known(Outlives::Type(component, b), assumptions)
                            })
                    }
                };
                if !valid {
                    errors.push((
                        file,
                        range,
                        "associated family equality lifetime prerequisites are not satisfied"
                            .into(),
                    ));
                    break;
                }
            }
        }
    };
    for (id, data) in items.functions.iter().skip(from.functions as usize) {
        if let (Some(projections), Some(regions)) =
            (sigs.fn_projections.get(id), sigs.regions.fns.get(id))
        {
            check(
                data.loc.file,
                data.sig_range,
                projections,
                &regions.outlives,
            );
        }
    }
    for (id, data) in items.structs.iter().skip(from.structs as usize) {
        if let (Some(projections), Some(regions)) = (
            sigs.struct_projections.get(id),
            sigs.regions.structs.get(id),
        ) {
            check(
                data.loc.file,
                data.def_range,
                projections,
                &regions.outlives,
            );
        }
    }
    for (id, data) in items.impls.iter().skip(from.impls as usize) {
        if let (Some(projections), Some(regions)) =
            (sigs.impl_projections.get(id), sigs.regions.impls.get(id))
        {
            check(data.file, data.def_range, projections, &regions.outlives);
        }
    }
    for (id, data) in items.traits.iter().skip(from.traits as usize) {
        if let (Some(signature), Some(regions)) = (sigs.traits.get(id), sigs.regions.traits.get(id))
        {
            check(
                data.loc.file,
                data.def_range,
                &signature.header_projections,
                &regions.outlives,
            );
        }
    }
}

/// Infer the required Self bound from method inputs at every occurrence of
/// one family. Rust takes their intersection, so a use with no such guarantee
/// cannot force a stronger bound onto an otherwise general family.
fn check_required_family_bounds(
    items: &ItemTree,
    sigs: &Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    fn visit(
        items: &ItemTree,
        ty: Ty,
        row: &[Region],
        next: &mut usize,
        out: &mut Vec<(crate::AliasTy, Vec<Region>)>,
    ) {
        if let TyKind::Alias(alias) = ty.kind()
            && has_projection_inputs(items, alias)
        {
            let start = *next;
            let count = slots(items, ty);
            if let Some(table) = row.get(start..start + count) {
                out.push((alias, table.to_vec()));
            }
            *next += count;
            return;
        }
        let own = match ty.kind() {
            TyKind::Ref(..) => 1,
            TyKind::Dynamic(object) => super::object_own_slots(items, object),
            TyKind::Adt(adt, _) => items.structs[adt.strukt].generics.lifetimes.len(),
            TyKind::Opaque(o) => o.lifetimes as usize,
            TyKind::FnPtr(_) => {
                *next += slots(items, ty);
                return;
            }
            _ => 0,
        };
        *next += own;
        for child in ty.children() {
            visit(items, child, row, next, out);
        }
    }
    for (id, data) in items.traits.iter().skip(from.traits as usize) {
        for family in &data.types {
            if family.generics.lifetimes.is_empty() {
                continue;
            }
            let mut required: Option<Vec<bool>> = None;
            for &function in &data.functions {
                let (Some(signature), Some(regions)) =
                    (sigs.fns.get(function), sigs.regions.fns.get(function))
                else {
                    continue;
                };
                let mut implied = Vec::new();
                let mut occurrences = Vec::new();
                for (&ty, row) in signature.params.iter().zip(regions.params.iter()) {
                    implied_outlives(items, sigs, ty, row, &mut implied);
                    visit(items, ty, row, &mut 0, &mut occurrences);
                }
                visit(items, signature.ret, &regions.ret, &mut 0, &mut occurrences);
                for (alias, row) in occurrences {
                    if alias.trait_ != id
                        || alias.name != family.name
                        || !matches!(alias.args.first().map(|t|t.kind()),Some(TyKind::Param(p))if p.index==0)
                    {
                        continue;
                    }
                    let at = alias.owner_lifetimes as usize
                        + alias.args.iter().map(|&t| slots(items, t)).sum::<usize>();
                    let mask: Vec<_> = (0..alias.family_lifetimes as usize)
                        .map(|j| {
                            row.get(at + j).is_some_and(|&r| {
                                outlives_known(Outlives::Type(alias.args[0], r), &implied)
                            })
                        })
                        .collect();
                    match &mut required {
                        Some(old) => {
                            for (a, b) in old.iter_mut().zip(mask) {
                                *a &= b
                            }
                        }
                        None => required = Some(mask),
                    }
                }
            }
            let Some(required) = required else { continue };
            let Some(owner) = sigs.regions.traits.get(id) else {
                continue;
            };
            let Some((_, contract)) = owner.families.iter().find(|(name, _)| *name == family.name)
            else {
                continue;
            };
            let assumptions: Vec<_> = owner
                .outlives
                .iter()
                .chain(contract.outlives.iter())
                .copied()
                .collect();
            for (index, is_required) in required.into_iter().enumerate() {
                if is_required
                    && !outlives_known(
                        Outlives::Type(
                            Ty::param(0, kw::SelfUpper),
                            Region::Param(contract.owner + index as u32),
                        ),
                        &assumptions,
                    )
                {
                    errors.push((
                        data.loc.file,
                        family.name_range,
                        "missing required `Self` lifetime bound on associated type".into(),
                    ));
                }
            }
        }
    }
}

/// Reconstruct every direct trait edge in the same order as TraitSig. Bound
/// lifetimes stay local to the edge rather than becoming owner parameters.
fn resolve_supertrait_regions(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    for (id, data) in items.traits.iter().skip(from.traits as usize) {
        let Some(signature) = sigs.traits.get(id) else {
            continue;
        };
        let self_ty = Ty::param(0, kw::SelfUpper);
        let mut written = data
            .supertraits
            .iter()
            .cloned()
            .map(|bound| flux_hir::WherePredicate {
                ty: TypeRef::Path(Path::single(kw::SelfUpper)),
                ty_range: bound.range,
                bound,
            })
            .collect::<Vec<_>>();
        let names = crate::generic_names(
            items,
            Some(flux_hir::Owner::Trait(id)),
            &Generics::default(),
        );
        let scope = TypeScope {
            self_ty: Some(self_ty),
            generics: &names,
            ..TypeScope::default()
        };
        for predicate in &data.generics.predicates {
            let mut scratch = Vec::new();
            if crate::resolve_type(
                items,
                data.loc.module,
                &predicate.ty,
                scope,
                predicate.ty_range,
                &mut scratch,
            ) == self_ty
            {
                written.push(predicate.clone());
            }
        }
        let predicates = signature
            .supertraits
            .iter()
            .map(|tr| tr.with_self(self_ty))
            .collect::<Vec<_>>();
        let owner = sigs.regions.traits[id]
            .names
            .iter()
            .map(|n| n.name)
            .collect::<Vec<_>>();
        let schema = Generics {
            predicates: written.into(),
            ..Generics::default()
        };
        let mut local = Vec::new();
        let (_, tables, _) = bound_regions(
            items,
            data.loc.module,
            &schema,
            &owner,
            &names,
            &predicates,
            &[],
            &mut local,
        );
        if tables.len() != predicates.len() {
            local.push((
                data.def_range,
                "supertrait region metadata is unavailable".into(),
            ));
        }
        errors.extend(
            local
                .into_iter()
                .map(|(range, message)| (data.loc.file, range, message)),
        );
        if let Some(regions) = sigs.regions.traits.get_mut(id) {
            regions.supertraits = tables;
        }
    }
}

/// The slots of each of a list of types.
type Tables = Box<[Box<[Region]>]>;

/// Resolves the lifetimes of the signatures of `items` from `from` on,
/// whose types `sigs` has, into `sigs.regions`, reporting what is wrong
/// with them.
pub(crate) fn resolve_regions(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    reported: &mut Vec<flux_hir::Diagnostic>,
) {
    let mut errors: Errors = Vec::new();
    let push = |errors: &mut Errors, file, mut list: Vec<(TextRange, String)>| {
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (file, r, m)));
    };
    // Resolve owner regions before the declaration admission query runs.
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let mut list = Vec::new();
        check_bounds(
            items,
            t.loc.module,
            &t.generics,
            t.supertraits.iter(),
            items.is_core(t.loc.module),
            !t.generics.lifetimes.is_empty(),
            &mut list,
        );
        for ty in &t.types {
            check_bounds(
                items,
                t.loc.module,
                &Generics::default(),
                ty.bounds.iter(),
                items.is_core(t.loc.module),
                true,
                &mut list,
            );
        }
        let names: Box<[LifetimeName]> = own_names(&t.generics).collect();
        let named: Vec<Option<Symbol>> = names.iter().map(|n| n.name).collect();
        let generics = crate::generic_names(
            items,
            Some(flux_hir::Owner::Trait(id)),
            &Generics::default(),
        );
        let mut outlives = Vec::new();
        bounds_outlives(
            items,
            sigs,
            t.loc.module,
            &t.generics,
            &named,
            &generics,
            Some(Ty::param(0, kw::SelfUpper)),
            None,
            &mut outlives,
            &mut list,
        );
        dedup(&mut outlives);
        sigs.regions.traits.insert(
            id,
            TraitRegions {
                supertraits: Box::default(),
                names,
                outlives: outlives.into(),
                families: Box::default(),
            },
        );
        push(&mut errors, t.loc.file, list);
    }
    resolve_supertrait_regions(items, sigs, from, &mut errors);
    resolve_trait_families(items, sigs, from, &mut errors);
    inherit_static_self(items, sigs, from);
    structs(items, sigs, from, &mut errors);
    impls(items, sigs, from, &mut errors);
    resolve_impl_families(items, sigs, from, &mut errors);
    // Concrete field projections can only map their occurrence regions once
    // the selected impl's header and associated value tables exist.
    structs_selected(items, sigs, from, &mut errors);
    functions(items, sigs, from, &mut errors);
    check_required_family_bounds(items, sigs, from, &mut errors);
    check_family_equality_outlives(items, sigs, from, &mut errors);
    // Constants and statics: elided lifetimes are `'static`.
    for (id, konst) in items.consts.iter().skip(from.consts as usize) {
        let Some(&ty) = sigs.consts.get(id) else {
            continue;
        };
        let names: Vec<Option<Symbol>> = match konst.owner {
            Some(flux_hir::Owner::Impl(imp)) => sigs
                .regions
                .impls
                .get(imp)
                .map_or(Vec::new(), |r| r.names.iter().map(|n| n.name).collect()),
            _ => Vec::new(),
        };
        let mut walker = Walker::new(items, konst.loc.module, names);
        walker.elided = Elided::Is(Region::Static);
        let regions = walker.regions(&konst.ty, ty);
        push(
            &mut errors,
            konst.loc.file,
            std::mem::take(&mut walker.errors),
        );
        sigs.regions.consts.insert(id, regions);
    }
    // Type aliases: their own errors.
    for (id, alias) in items.type_aliases.iter().skip(from.type_aliases as usize) {
        let mut list = Vec::new();
        alias_regions(items, id, &mut list);
        push(&mut errors, alias.loc.file, list);
    }
    reported.extend(
        errors
            .into_iter()
            .map(|(file, range, message)| flux_hir::Diagnostic {
                file,
                range,
                message,
            }),
    );
}

/// Structs' and enums' fields, variances and what their parameters
/// outlive.
fn structs(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    let from = from.structs as usize;
    for (id, s) in items.structs.iter().skip(from) {
        let names: Vec<Option<Symbol>> =
            s.generics.lifetimes.iter().map(|l| Some(l.name)).collect();
        let identity: Vec<Region> = (0..names.len() as u32).map(Region::Param).collect();
        let generics: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let mut walker = Walker::new(items, s.loc.module, names);
        walker.self_regions = Some(&identity);
        walker.scope = TypeScope {
            generics: &generics,
            signature_of: Some("structs"),
            ..TypeScope::default()
        };
        walker.add_projection_bounds(&s.generics);
        walker.elided = Elided::Missing(None);
        let fields: Box<[Box<[Region]>]> = crate::all_fields(s)
            .zip(sigs.structs[id].iter())
            .map(|(f, &ty)| walker.regions(&f.ty, ty))
            .collect();
        let mut list = std::mem::take(&mut walker.errors);
        check_bounds(
            items,
            s.loc.module,
            &s.generics,
            std::iter::empty(),
            items.is_core(s.loc.module),
            true,
            &mut list,
        );
        let names: Vec<_> = s.generics.lifetimes.iter().map(|l| Some(l.name)).collect();
        let (bound_selfs, bound_traits, projections) = bound_regions(
            items,
            s.loc.module,
            &s.generics,
            &names,
            &generics,
            sigs.struct_predicates.get(id).map_or(&[][..], |p| p),
            sigs.struct_projections.get(id).map_or(&[][..], |p| p),
            &mut list,
        );
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (s.loc.file, r, m)));
        let count = s.generics.lifetimes.len() + s.generics.params.len();
        sigs.regions.structs.insert(
            id,
            StructRegions {
                selected_inputs: Box::default(),
                bound_selfs,
                bound_traits,
                projections,
                lifetimes: s.generics.lifetimes.len() as u32,
                fields,
                // Mutation through a shared reference makes the payload
                // invariant, despite storing `T` directly in these core types.
                variances: if flux_solve::is_lang_struct(items, id, "unsafe_cell")
                    || flux_solve::is_lang_struct(items, id, "cell")
                    || flux_solve::is_lang_struct(items, id, "ref_cell")
                {
                    vec![Variance::Invariant; count].into()
                } else if flux_solve::is_lang_struct(items, id, "phantom_data") {
                    // `PhantomData<T>` acts as if it held a `T`.
                    vec![Variance::Covariant; count].into()
                } else {
                    vec![Variance::Bivariant; count].into()
                },
                outlives: Box::default(),
            },
        );
    }
    struct_variances(items, sigs, from);
    struct_outlives(items, sigs, from, errors);
    for (id, data) in items.structs.iter().skip(from) {
        let metadata = &sigs.regions.structs[id];
        let mut assumptions = metadata.outlives.to_vec();
        let mut list = Vec::new();
        check_bound_wf(
            items,
            sigs,
            &data.generics,
            sigs.struct_predicates.get(id).map_or(&[][..], |p| p),
            &metadata.bound_selfs,
            &metadata.bound_traits,
            &mut assumptions,
            &mut list,
        );
        errors.extend(list.into_iter().map(|(r, m)| (data.loc.file, r, m)));
    }
}

fn structs_selected(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    let start = from.structs as usize;
    let mut changed = false;
    for (id, s) in items.structs.iter().skip(start) {
        let old = sigs.structs[id];
        let fields = Ty::list(
            &crate::all_fields(s)
                .zip(old)
                .map(|(field, &ty)| super::selected::field_type(items, sigs, &field.ty, ty))
                .collect::<Vec<_>>(),
        );
        if fields == old {
            continue;
        }
        let names = s.generics.lifetimes.iter().map(|l| Some(l.name)).collect();
        let identity: Vec<_> = (0..s.generics.lifetimes.len() as u32)
            .map(Region::Param)
            .collect();
        let generics: Vec<_> = s.generics.params.iter().map(|p| p.name).collect();
        let consts = crate::generic_consts(items, None, &s.generics);
        let bounds = sigs.struct_predicates.get(id).map_or(&[][..], |p| p);
        let mut walker = Walker::new(items, s.loc.module, names);
        walker.signatures = Some(sigs);
        walker.self_regions = Some(&identity);
        walker.scope = TypeScope {
            self_ty: Some(Ty::adt(id, s.name, &crate::params_of(&s.generics, 0))),
            generics: &generics,
            consts: &consts,
            bounds,
            ..TypeScope::default()
        };
        walker.elided = Elided::Missing(None);
        let rows = crate::all_fields(s)
            .zip(fields)
            .map(|(field, &ty)| walker.regions(&field.ty, ty))
            .collect();
        errors.extend(
            walker
                .errors
                .into_iter()
                .map(|(range, message)| (s.loc.file, range, message)),
        );
        let mut metadata = sigs.regions.structs[id].clone();
        metadata.fields = rows;
        // Recompute the fixed point from unused parameters, rather than
        // retaining variance inferred from a now-normalized projection.
        metadata.variances =
            vec![Variance::Bivariant; s.generics.lifetimes.len() + s.generics.params.len()].into();
        metadata.selected_inputs = walker.selected_inputs.into();
        sigs.structs.insert(id, fields);
        sigs.regions.structs.insert(id, metadata);
        changed = true;
    }
    if changed {
        struct_variances(items, sigs, start);
        struct_outlives(items, sigs, start, errors);
    }
    for (id, s) in items.structs.iter().skip(start) {
        if sigs.structs[id].contains(&Ty::ERROR) {
            continue;
        }
        let regions = &sigs.regions.structs[id];
        for (i, l) in s.generics.lifetimes.iter().enumerate() {
            if regions.variances[i] == Variance::Bivariant {
                errors.push((
                    s.loc.file,
                    l.range,
                    format!("lifetime parameter `{}` is never used", l.name),
                ));
            }
        }
    }
}

/// `impl` blocks' headers and associated types.
fn impls(items: &ItemTree, sigs: &mut Signatures, from: flux_hir::ItemCounts, errors: &mut Errors) {
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let names: Vec<Option<Symbol>> = imp
            .generics
            .lifetimes
            .iter()
            .map(|l| Some(l.name))
            .collect();
        let generics: Vec<Symbol> = imp.generics.params.iter().map(|p| p.name).collect();
        let self_ty = sigs.impls[id];
        let mut walker = Walker::new(items, imp.module, names);
        walker.scope = TypeScope {
            generics: &generics,
            signature_of: Some("implementations"),
            ..TypeScope::default()
        };
        walker.add_projection_bounds(&imp.generics);
        walker.elided = Elided::Fresh;
        walker.selections = Some(Vec::new());
        let self_regions = walker.regions(&imp.self_ty, self_ty);
        let mut header_selections = vec![walker.selections.take().unwrap().into_boxed_slice()];
        let trait_ref = sigs.impl_traits.get(id).copied();
        let mut trait_args: Vec<Box<[Region]>> = Vec::new();
        let mut trait_lifetimes: Box<[Region]> = Box::default();
        if let (Some(trait_ref), Some(bound)) = (trait_ref, &imp.trait_ref) {
            let last = bound.path.segments.len().saturating_sub(1);
            let written = bound.path.args(last);
            // Unlike self types, an impl's trait path cannot omit its
            // lifetime arguments (E0726); explicit `'_` introduces a fresh one.
            let mut lifetimes = Vec::new();
            walker.trait_lifetimes(&bound.path, trait_ref.trait_, true, &mut lifetimes);
            trait_lifetimes = lifetimes.into();
            for (i, &arg) in trait_ref.args.iter().enumerate() {
                walker.selections = Some(Vec::new());
                let regions = match written
                    .filter(|w| !w.parenthesized)
                    .and_then(|w| w.types.get(i))
                {
                    Some(t) => walker.regions(t, arg),
                    None if arg == self_ty && trait_default_self(items, trait_ref.trait_, i) => {
                        walker.selections = Some(header_selections[0].to_vec());
                        self_regions.clone()
                    }
                    None => {
                        if slots(items, arg) != 0 {
                            walker.errors.push((
                                bound.range,
                                "lifetimes in default trait type arguments are not supported yet"
                                    .to_owned(),
                            ));
                        }
                        vec![Region::Erased; slots(items, arg)].into()
                    }
                };
                trait_args.push(regions);
                header_selections.push(walker.selections.take().unwrap().into_boxed_slice());
            }
        }
        let fresh = std::mem::take(&mut walker.fresh);
        let mut all_names: Vec<LifetimeName> = own_names(&imp.generics).collect();
        all_names.extend(
            fresh
                .iter()
                .map(|&range| LifetimeName { name: None, range }),
        );
        walker.names = all_names.iter().map(|n| n.name).collect();
        walker.self_regions = Some(&self_regions);
        walker.scope.self_ty = Some(self_ty);
        walker.self_projection_owner = trait_ref.map(|tr| (tr.trait_, trait_lifetimes.clone()));
        walker.elided = Elided::Missing(None);
        let mut type_selections = Vec::new();
        let types: Box<[(Symbol, Box<[Region]>)]> = imp
            .types
            .iter()
            .map(|t| {
                if !t.generics.is_empty() || !t.generics.lifetimes.is_empty() {
                    return (t.name, Box::default());
                }
                let ty = flux_solve::traits::impl_type(sigs, id, t.name).unwrap_or(Ty::ERROR);
                walker.selections = Some(Vec::new());
                let regions = walker.regions(&t.ty, ty);
                type_selections
                    .push((t.name, walker.selections.take().unwrap().into_boxed_slice()));
                (t.name, regions)
            })
            .collect();
        let mut list = std::mem::take(&mut walker.errors);
        // rustc's E0207: an associated type may name only lifetimes the
        // header or the bounds' associated types decide.
        let mut in_bounds = Vec::new();
        for p in imp.generics.predicates.iter() {
            for args in p.bound.path.args.iter().flatten() {
                for b in args.bindings.iter() {
                    lifetimes_in(&b.ty, &mut in_bounds);
                }
            }
        }
        for (i, l) in imp.generics.lifetimes.iter().enumerate() {
            let r = Region::Param(i as u32);
            let decided = self_regions
                .iter()
                .chain(trait_lifetimes.iter())
                .chain(trait_args.iter().flat_map(|a| a.iter()))
                .any(|&s| s == r)
                || in_bounds.iter().any(|b| b.name == l.name);
            if !decided && types.iter().any(|(_, t)| t.contains(&r)) {
                list.push((
                    l.range,
                    format!(
                        "the lifetime parameter `{}` is not constrained by the impl trait, self type, or predicates",
                        l.name
                    ),
                ));
            }
        }
        let names = walker.names.clone();
        check_bounds(
            items,
            imp.module,
            &imp.generics,
            std::iter::empty(),
            items.is_core(imp.module),
            true,
            &mut list,
        );
        let mut outlives = Vec::new();
        bounds_outlives(
            items,
            sigs,
            imp.module,
            &imp.generics,
            &names,
            &generics,
            Some(self_ty),
            Some(&self_regions),
            &mut outlives,
            &mut list,
        );
        dedup(&mut outlives);
        let bounds: Box<[Outlives<Region>]> = outlives.clone().into();
        implied_outlives_with_static(
            items,
            sigs,
            self_ty,
            &self_regions,
            Some(Region::Static),
            &mut outlives,
        );
        // As with the self type, Rust assumes the header's argument types
        // are well formed. The trait declaration's explicit bounds still
        // need proof, e.g. `impl Tr<'a, &'b T>` cannot assume `'b: 'a`.
        if let Some(trait_ref) = trait_ref {
            for (&arg, regions) in trait_ref.args.iter().zip(&trait_args) {
                implied_outlives_with_static(
                    items,
                    sigs,
                    arg,
                    regions,
                    Some(Region::Static),
                    &mut outlives,
                );
            }
        }
        dedup(&mut outlives);
        let (bound_selfs, bound_traits, projections) = bound_regions(
            items,
            imp.module,
            &imp.generics,
            &names,
            &generics,
            sigs.impl_predicates.get(id).map_or(&[][..], |p| p),
            sigs.impl_projections.get(id).map_or(&[][..], |p| p),
            &mut list,
        );
        check_bound_wf(
            items,
            sigs,
            &imp.generics,
            sigs.impl_predicates.get(id).map_or(&[][..], |p| p),
            &bound_selfs,
            &bound_traits,
            &mut outlives,
            &mut list,
        );
        if let (Some(trait_ref), Some(bound)) = (trait_ref, &imp.trait_ref) {
            let predicate = crate::Predicate { self_ty, trait_ref };
            let regions = TraitRefRegions {
                bound: 0,
                lifetimes: trait_lifetimes.clone(),
                args: trait_args.clone().into(),
            };
            check_trait_wf(
                items,
                sigs,
                &predicate,
                &self_regions,
                &regions,
                &outlives,
                bound.range,
                &mut list,
            );
        }
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (imp.file, r, m)));
        sigs.regions.impls.insert(
            id,
            ImplRegions {
                families: Box::default(),
                names: all_names.into(),
                self_ty: self_regions,
                trait_args: trait_args.into(),
                header_selections: header_selections.into(),
                trait_lifetimes,
                types,
                type_selections: type_selections.into(),
                outlives: outlives.into(),
                bounds,
                bound_selfs,
                bound_traits,
                projections,
            },
        );
    }
}

/// Rebase the selected impl's explicitly represented header lifetimes onto
/// this impl's Self slots. Substitutions that introduce additional type slots
/// need a richer map and are rejected instead of receiving elided lifetimes.
fn inherited_self_regions(
    items: &ItemTree,
    sigs: &Signatures,
    goal: crate::Predicate,
    actual: &[Region],
    name: Symbol,
) -> Option<Box<[Region]>> {
    let (imp, args) = flux_solve::traits::select_impl(items, sigs, goal)?;
    let header = sigs.impls[imp];
    let regions = sigs.regions.impls.get(imp)?;
    if header.subst(&args) != goal.self_ty
        || slots(items, header) != slots(items, goal.self_ty)
        || regions.self_ty.len() != actual.len()
        || regions.trait_args.iter().any(|r| !r.is_empty())
        || !regions.trait_lifetimes.is_empty()
    {
        return None;
    }
    let mut map = vec![None; regions.names.len()];
    for (&pattern, &target) in regions.self_ty.iter().zip(actual) {
        if !matches!(target, Region::Static | Region::Param(_)) {
            return None;
        }
        match pattern {
            Region::Param(i) => {
                let entry = map.get_mut(i as usize)?;
                if entry.is_some_and(|previous| previous != target) {
                    return None;
                }
                *entry = Some(target);
            }
            Region::Static if target == Region::Static => {}
            _ => return None,
        }
    }
    let ty = flux_solve::traits::impl_type(sigs, imp, name)?;
    let table = regions.types.iter().find(|(n, _)| *n == name)?.1.as_ref();
    if slots(items, ty) != table.len() || slots(items, ty.subst(&args)) != table.len() {
        return None;
    }
    table
        .iter()
        .map(|r| match *r {
            Region::Static => Some(Region::Static),
            Region::Param(i) => map.get(i as usize).copied().flatten(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()
        .map(Into::into)
}

/// Reconstruct the represented contract of a safe trait method with one
/// owner lifetime, type-only locals and no associated/opaque signature types.
/// Admission compares this independent result with every stored table.
pub(super) fn safe_method_contract(
    items: &ItemTree,
    sigs: &Signatures,
    id: flux_hir::FunctionId,
) -> Option<(crate::FnSig, FnRegions, crate::traits::Bounds)> {
    let function = items.functions.get(id)?;
    let Some(flux_hir::Owner::Trait(owner)) = function.owner else {
        return None;
    };
    let declaration = items.traits.get(owner)?;
    let owner_names: Vec<_> = own_names(&declaration.generics).collect();
    let names = crate::generic_names(items, function.owner, &function.generics);
    let self_ty = Ty::param(0, kw::SelfUpper);
    let owner_bounds = flux_solve::traits::owner_predicates(items, sigs, function.owner);
    let lens = crate::ArrayLens {
        values: &sigs.array_lens,
        missing: Default::default(),
        arg_tys: &sigs.arg_tys,
        new_arg_tys: Default::default(),
    };
    let scope = TypeScope {
        self_ty: Some(self_ty),
        generics: &names,
        bounds: &owner_bounds,
        signature_of: Some("functions"),
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        lens: Some(&lens),
        ..TypeScope::default()
    };
    let mut errors = Vec::new();
    let bounds = crate::resolve_predicates(
        items,
        sigs,
        function.loc.module,
        &function.generics,
        scope,
        &mut errors,
    );
    let environment: Vec<_> = owner_bounds
        .iter()
        .chain(&bounds.predicates)
        .copied()
        .collect();
    let environment = flux_solve::traits::elaborate(sigs, &environment);
    let scope = TypeScope {
        bounds: &environment,
        ..scope
    };
    let signature = crate::FnSig {
        params: function
            .params
            .iter()
            .map(|parameter| {
                crate::resolve_type(
                    items,
                    function.loc.module,
                    &parameter.ty,
                    scope,
                    parameter.range,
                    &mut errors,
                )
            })
            .collect(),
        ret: crate::resolve_type(
            items,
            function.loc.module,
            &function.ret,
            scope,
            function.ret_range,
            &mut errors,
        ),
    };
    let mut walker = Walker::new(
        items,
        function.loc.module,
        owner_names
            .iter()
            .map(|name| name.name)
            .chain(function.generics.lifetimes.iter().map(|l| Some(l.name)))
            .collect(),
    );
    walker.scope = scope;
    walker.self_projection_owner = Some((
        owner,
        (0..owner_names.len() as u32).map(Region::Param).collect(),
    ));
    walker.add_projection_bounds(&declaration.generics);
    walker.add_projection_bounds(&function.generics);
    walker.elided = Elided::Fresh;
    let mut elision_params = Vec::new();
    let capture_selections = signature
        .params
        .iter()
        .chain([&signature.ret])
        .any(|ty| ty.has_aliases());
    let mut selected_inputs = Vec::new();
    let params: Box<[Box<[Region]>]> = function
        .params
        .iter()
        .zip(signature.params.iter())
        .map(|(written, &ty)| {
            elision_params.push(walker.elision_regions(&written.ty, ty));
            walker.selections = capture_selections.then(Vec::new);
            let row = walker.regions(&written.ty, ty);
            if let Some(selections) = walker.selections.take() {
                selected_inputs.push(selections.into_boxed_slice());
            }
            row
        })
        .collect();
    let mut distinct = Vec::new();
    for row in &elision_params {
        let mut own = Vec::new();
        for &region in row {
            if !own.contains(&region) {
                own.push(region);
            }
        }
        distinct.extend(own);
    }
    walker.elided = if function.has_self
        && matches!(
            function.params.first().map(|parameter| &parameter.ty),
            Some(TypeRef::Ref { .. })
        ) {
        Elided::Is(*params.first()?.first()?)
    } else if distinct.len() == 1 {
        Elided::Is(distinct[0])
    } else {
        Elided::Missing(None)
    };
    walker.selections = capture_selections.then(Vec::new);
    let ret = walker.regions(&function.ret, signature.ret);
    if let Some(selections) = walker.selections.take() {
        selected_inputs.push(selections.into_boxed_slice());
    }
    let mut all_names = owner_names;
    all_names.extend(function.generics.lifetimes.iter().map(|l| LifetimeName {
        name: Some(l.name),
        range: l.range,
    }));
    all_names.extend(
        walker
            .fresh
            .iter()
            .map(|&range| LifetimeName { name: None, range }),
    );
    errors.append(&mut walker.errors);
    let region_names: Vec<_> = all_names.iter().map(|name| name.name).collect();
    let mut outlives = trait_outlives_contract(items, sigs, owner)?.to_vec();
    bounds_outlives(
        items,
        sigs,
        function.loc.module,
        &function.generics,
        &region_names,
        &names,
        Some(self_ty),
        None,
        &mut outlives,
        &mut errors,
    );
    for (&ty, row) in signature.params.iter().zip(params.iter()) {
        implied_outlives_with_static(items, sigs, ty, row, Some(Region::Static), &mut outlives);
    }
    implied_outlives_with_static(
        items,
        sigs,
        signature.ret,
        &ret,
        Some(Region::Static),
        &mut outlives,
    );
    outlives.retain(|bound| match *bound {
        Outlives::Region(a, b) => a != Region::Erased && b != Region::Erased && a != b,
        Outlives::Type(_, b) => b != Region::Erased,
    });
    dedup(&mut outlives);
    let (bound_selfs, bound_traits, projections) = bound_regions(
        items,
        function.loc.module,
        &function.generics,
        &region_names,
        &names,
        &bounds.predicates,
        &bounds.projections,
        &mut errors,
    );
    check_bound_wf(
        items,
        sigs,
        &function.generics,
        &bounds.predicates,
        &bound_selfs,
        &bound_traits,
        &mut outlives,
        &mut errors,
    );
    if !errors.is_empty()
        || !lens.missing.borrow().is_empty()
        || !lens.new_arg_tys.borrow().is_empty()
    {
        return None;
    }
    let mut regions = FnRegions {
        owner: declaration.generics.lifetimes.len() as u32,
        names: all_names.into(),
        params,
        ret,
        selected_inputs: selected_inputs.into(),
        outlives: outlives.into(),
        has_bounds: !function.generics.outlives.is_empty()
            || !function.generics.type_outlives.is_empty(),
        callback_binder: CallbackBinder::Unclassified,
        bound_selfs,
        bound_traits,
        projections,
        opaques: Box::default(),
    };
    regions.callback_binder = callback_binder(items, sigs, id, &regions);
    Some((signature, regions, bounds))
}

/// Functions' parameters and return types, with elision, and their
/// bounds.
fn functions(
    items: &ItemTree,
    sigs: &mut Signatures,
    from: flux_hir::ItemCounts,
    errors: &mut Errors,
) {
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let mut owner_names: Vec<LifetimeName> = Vec::new();
        let mut owner_outlives: Vec<Outlives<Region>> = Vec::new();
        let mut self_regions: Option<Box<[Region]>> = None;
        let mut self_types: Box<[(Symbol, Box<[Region]>)]> = Box::default();
        if let Some(flux_hir::Owner::Impl(imp)) = function.owner
            && let Some(r) = sigs.regions.impls.get(imp)
        {
            owner_names = r.names.to_vec();
            owner_outlives = r.outlives.to_vec();
            self_regions = Some(r.self_ty.clone());
            self_types = r.types.clone();
        }
        if let Some(flux_hir::Owner::Trait(trait_)) = function.owner
            && let Some(r) = sigs.regions.traits.get(trait_)
        {
            owner_names = r.names.to_vec();
            owner_outlives = r.outlives.to_vec();
        }
        // Own associated definitions already have tables in this impl's
        // lifetime namespace. Inherited definitions must be mapped from the
        // selected supertrait impl, never elided as a borrow of the receiver.
        let mut unmapped_self_types = Vec::new();
        let mut inherited_self_types = Vec::new();
        if let Some(flux_hir::Owner::Impl(imp)) = function.owner {
            let self_ty = sigs.impls[imp];
            let candidates = flux_solve::traits::associated_self_bounds(
                sigs,
                sigs.impl_traits.get(imp).copied(),
                Some(self_ty),
            );
            let mut tables = Vec::new();
            for candidate in candidates.iter().skip(1) {
                for associated in &items.traits[candidate.trait_ref.trait_].types {
                    if self_types
                        .iter()
                        .chain(tables.iter())
                        .any(|(name, _)| *name == associated.name)
                    {
                        continue;
                    }
                    match inherited_self_regions(
                        items,
                        sigs,
                        *candidate,
                        self_regions.as_deref().unwrap_or(&[]),
                        associated.name,
                    ) {
                        Some(regions) => tables.push((associated.name, regions)),
                        None => unmapped_self_types.push(associated.name),
                    }
                }
            }
            inherited_self_types = tables;
        }
        let owner_count = owner_names.len() as u32;
        let mut names: Vec<Option<Symbol>> = owner_names.iter().map(|n| n.name).collect();
        names.extend(function.generics.lifetimes.iter().map(|l| Some(l.name)));
        let generics = crate::generic_names(items, function.owner, &function.generics);
        let self_ty = crate::self_ty(sigs, function.owner).or_else(|| {
            function.ctor.map(|(strukt, _)| {
                Ty::adt(
                    strukt,
                    items.structs[strukt].name,
                    &crate::params_of(&function.generics, 0),
                )
            })
        });
        if function.ctor.is_some() && self_regions.is_none() {
            self_regions = Some(
                (0..function.generics.lifetimes.len() as u32)
                    .map(Region::Param)
                    .collect(),
            );
        }
        let mut walker = Walker::new(items, function.loc.module, names);
        walker.self_regions = self_regions.as_deref();
        walker.self_types = &self_types;
        walker.unmapped_self_types = &unmapped_self_types;
        walker.inherited_self_types = &inherited_self_types;
        walker.scope = TypeScope {
            self_ty,
            generics: &generics,
            signature_of: Some("functions"),
            ..TypeScope::default()
        };
        walker.self_projection_owner = match function.owner {
            Some(flux_hir::Owner::Trait(owner)) => {
                Some((owner, (0..owner_count).map(Region::Param).collect()))
            }
            Some(flux_hir::Owner::Impl(owner)) => sigs.impl_traits.get(owner).and_then(|tr| {
                sigs.regions
                    .impls
                    .get(owner)
                    .map(|r| (tr.trait_, r.trait_lifetimes.clone()))
            }),
            None => None,
        };
        for generics in items
            .owner_generics(function.owner)
            .into_iter()
            .chain([&function.generics])
        {
            walker.add_projection_bounds(generics);
        }
        walker.elided = Elided::Fresh;
        let (sig_params, mut sig_ret) = (sigs.fns[id].params.clone(), sigs.fns[id].ret);
        let mut elision_params = Vec::new();
        let selected_method = matches!(function.owner, Some(flux_hir::Owner::Impl(imp))
            if sigs.impl_traits.get(imp).is_some());
        let capture_selections = selected_method
            || sig_params
                .iter()
                .chain([&sig_ret])
                .any(|ty| ty.has_aliases());
        let mut selected_inputs = Vec::new();
        walker.qualified_legacy_selections = selected_method;
        let params: Box<[Box<[Region]>]> = function
            .params
            .iter()
            .zip(sig_params.iter())
            .map(|(p, &ty)| {
                elision_params.push(walker.elision_regions(&p.ty, ty));
                walker.selections = capture_selections.then(Vec::new);
                let row = walker.regions(&p.ty, ty);
                if let Some(selections) = walker.selections.take() {
                    selected_inputs.push(selections.into_boxed_slice());
                }
                row
            })
            .collect();
        // Elision in the return type.
        let first_fresh = walker.names.len() as u32;
        let self_ref = function.has_self
            && matches!(
                function.params.first().map(|p| &p.ty),
                Some(TypeRef::Ref { .. })
            );
        let inputs: Vec<Region> = elision_params
            .iter()
            .flat_map(|p| p.iter().copied())
            .collect();
        // Each parameter's lifetimes, each once, as rustc counts them:
        // `&'a S<'a>`, and `&dyn Tr`, whose lifetime bound is its
        // reference's, have one; `x: &'a i32, y: &'a i32` two.
        let mut distinct: Vec<Region> = Vec::new();
        for p in elision_params.iter() {
            let mut own: Vec<Region> = Vec::new();
            for &r in p.iter() {
                if !own.contains(&r) {
                    own.push(r);
                }
            }
            distinct.extend(own);
        }
        walker.elided = match (self_ref, params.first()) {
            (true, Some(first)) if !first.is_empty() => Elided::Is(first[0]),
            _ if distinct.len() == 1 => Elided::Is(distinct[0]),
            _ => {
                let fresh = |r: &&Region| matches!(r, Region::Param(i) if *i >= first_fresh);
                let per_param: Vec<(Option<Symbol>, usize)> = function
                    .params
                    .iter()
                    .zip(params.iter())
                    .map(|(p, regions)| (p.name, regions.iter().filter(fresh).count()))
                    .collect();
                let named = inputs.iter().any(|r| !fresh(&r));
                Elided::Missing(Some(elision_help(&per_param, named)))
            }
        };
        let opaque_sigs = sigs.opaques.get(id).cloned().unwrap_or_default();
        let opaques = opaque_regions(function, &opaque_sigs, &mut walker);
        if !opaques.is_empty() {
            // Its opaque types' slots are what they capture.
            sig_ret = sig_ret.map_opaques(&mut |o| {
                let lifetimes = match opaques.get(o.index as usize) {
                    Some(r) if o.function == id => r.captures.len() as u32,
                    _ => o.lifetimes,
                };
                Ty::opaque(crate::OpaqueTy { lifetimes, ..o })
            });
            if let Some(sig) = sigs.fns.get_mut(id) {
                sig.ret = sig_ret;
            }
            walker.opaque_captures = opaques.iter().map(|o| o.captures.clone()).collect();
        }
        walker.selections = capture_selections.then(Vec::new);
        let ret = walker.regions(&function.ret, sig_ret);
        if let Some(selections) = walker.selections.take() {
            selected_inputs.push(selections.into_boxed_slice());
        }
        walker.opaque_captures.clear();
        let fresh = std::mem::take(&mut walker.fresh);
        let mut all_names = owner_names;
        all_names.extend(own_names(&function.generics));
        all_names.extend(
            fresh
                .iter()
                .map(|&range| LifetimeName { name: None, range }),
        );
        let names: Vec<Option<Symbol>> = all_names.iter().map(|n| n.name).collect();
        let mut list = std::mem::take(&mut walker.errors);
        check_bounds(
            items,
            function.loc.module,
            &function.generics,
            std::iter::empty(),
            items.is_core(function.loc.module),
            true,
            &mut list,
        );
        let mut outlives = owner_outlives;
        bounds_outlives(
            items,
            sigs,
            function.loc.module,
            &function.generics,
            &names,
            &generics,
            self_ty,
            self_regions.as_deref(),
            &mut outlives,
            &mut list,
        );
        for (&ty, regions) in sig_params.iter().zip(params.iter()) {
            implied_outlives_with_static(
                items,
                sigs,
                ty,
                regions,
                Some(Region::Static),
                &mut outlives,
            );
        }
        implied_outlives_with_static(
            items,
            sigs,
            sig_ret,
            &ret,
            Some(Region::Static),
            &mut outlives,
        );
        outlives.retain(|p| match *p {
            Outlives::Region(a, b) => a != Region::Erased && b != Region::Erased && a != b,
            Outlives::Type(_, b) => b != Region::Erased,
        });
        dedup(&mut outlives);
        walker.names = names.clone();
        let (bound_selfs, bound_traits, projections) = bound_regions_with_walker(
            items,
            &function.generics,
            sigs.fn_predicates.get(id).map_or(&[][..], |p| p),
            sigs.fn_projections.get(id).map_or(&[][..], |p| p),
            &mut list,
            &mut walker,
            None,
        );
        check_bound_wf(
            items,
            sigs,
            &function.generics,
            sigs.fn_predicates.get(id).map_or(&[][..], |p| p),
            &bound_selfs,
            &bound_traits,
            &mut outlives,
            &mut list,
        );
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (function.loc.file, r, m)));
        let mut regions = FnRegions {
            names: all_names.into(),
            owner: owner_count,
            params,
            ret,
            selected_inputs: selected_inputs.into(),
            outlives: outlives.into(),
            has_bounds: !function.generics.outlives.is_empty()
                || !function.generics.type_outlives.is_empty(),
            callback_binder: CallbackBinder::Unclassified,
            bound_selfs,
            bound_traits,
            projections,
            opaques: opaques.into(),
        };
        regions.callback_binder = callback_binder(items, sigs, id, &regions);
        sigs.regions.fns.insert(id, regions);
    }
}

/// Classify the supported callback boundary from both source and resolved
/// tables. Source traversal is essential: aliases/projections can erase a
/// lifetime argument without making its occurrence in a predicate late-bound.
/// HIR has already elided its supported single Fn binder; unsupported named or
/// multiple binders are diagnosed by lowering and are not admitted here.
pub(super) fn callback_binder(
    items: &ItemTree,
    sigs: &Signatures,
    id: flux_hir::FunctionId,
    regions: &FnRegions,
) -> CallbackBinder {
    let function = &items.functions[id];
    let owner_names = match function.owner {
        Some(flux_hir::Owner::Impl(owner)) => sigs.regions.impls.get(owner).map(|r| &*r.names),
        Some(flux_hir::Owner::Trait(owner)) => sigs.regions.traits.get(owner).map(|r| &*r.names),
        None => Some(&[][..]),
    };
    let Some(owner_names) = owner_names else {
        return CallbackBinder::Unclassified;
    };
    if regions.owner as usize != owner_names.len()
        || regions.names.get(..owner_names.len()) != Some(owner_names)
    {
        return CallbackBinder::Unclassified;
    }
    let predicates = sigs.fn_predicates.get(id).map_or(&[][..], |p| p);
    let projections = sigs.fn_projections.get(id).map_or(&[][..], |p| p);
    if regions.owner as usize > regions.names.len()
        || predicates.len() != function.generics.predicates.len()
        || regions.bound_selfs.len() != predicates.len()
        || regions.bound_traits.len() != predicates.len()
        || regions.projections.len() != projections.len()
    {
        return CallbackBinder::Unclassified;
    }
    if regions.has_bounds
        != (!function.generics.outlives.is_empty() || !function.generics.type_outlives.is_empty())
    {
        return CallbackBinder::Unclassified;
    }
    // A type-only `T: 'static` condition must still be proved on reification,
    // but it does not make the independent input loans early-bound.
    let mut early = vec![false; regions.names.len()];
    let own = |r: Region| match r {
        Region::Param(i) if (i as usize) < regions.names.len() => Some(i >= regions.owner),
        Region::Param(_) => None,
        // Fn input slots are elided/Erased here, and Output can use Late.
        Region::Static | Region::Erased | Region::Late(_) => Some(false),
    };
    for ((p, self_table), trait_table) in predicates
        .iter()
        .zip(regions.bound_selfs.iter())
        .zip(regions.bound_traits.iter())
    {
        if self_table.len() != slots(items, p.self_ty)
            || trait_table.lifetimes.len()
                != items.traits[p.trait_ref.trait_].generics.lifetimes.len()
            || trait_table.args.len() != p.trait_ref.args.len()
            || trait_table
                .args
                .iter()
                .zip(p.trait_ref.args.iter())
                .any(|(table, &ty)| table.len() != slots(items, ty))
        {
            return CallbackBinder::Unclassified;
        }
    }
    if projections
        .iter()
        .zip(regions.projections.iter())
        .any(|(p, table)| table.len() != slots(items, p.ty))
    {
        return CallbackBinder::Unclassified;
    }
    for &r in regions
        .bound_selfs
        .iter()
        .flatten()
        .chain(
            regions
                .bound_traits
                .iter()
                .flat_map(|t| t.lifetimes.iter().chain(t.args.iter().flatten())),
        )
        .chain(regions.projections.iter().flatten())
    {
        let Some(is_own) = own(r) else {
            return CallbackBinder::Unclassified;
        };
        if is_own && let Region::Param(i) = r {
            early[i as usize] = true;
        }
    }
    let mut written = Vec::new();
    for p in &function.generics.predicates {
        let start = written.len();
        if !callback_type_lifetimes(&p.ty, &mut written)
            || !callback_path_lifetimes(&p.bound.path, &mut written)
        {
            return CallbackBinder::Unclassified;
        }
        let mut local = written.split_off(start);
        local.retain(|l| !p.bound.bound_lifetimes.contains(&l.name));
        written.extend(local);
    }
    for predicate in &function.generics.outlives {
        written.extend([predicate.longer, predicate.shorter]);
    }
    for predicate in &function.generics.type_outlives {
        written.push(predicate.lifetime);
        if !callback_type_lifetimes(&predicate.ty, &mut written) {
            return CallbackBinder::Unclassified;
        }
    }
    for lifetime in written {
        if lifetime.is_static() || lifetime.is_anonymous() {
            continue;
        }
        // Match Walker::named: nearest declaration wins, owner prefix first.
        // Supported Fn binder names have been removed by HIR, not confused
        // with this function's own names.
        let Some(i) = regions
            .names
            .iter()
            .rposition(|n| n.name == Some(lifetime.name))
        else {
            return CallbackBinder::Unclassified;
        };
        early[i] |= i >= regions.owner as usize;
    }
    // A predicate may fix one lifetime without fixing the signature's other
    // loans: `R: Read<'de>` does not bind an independent slice input `'s`.
    // Keep the conservative boundary whenever an early slot actually occurs
    // in the signature. Predicate-only slots remain callee obligations.
    let mut bound = regions
        .params
        .iter()
        .flatten()
        .chain(regions.ret.iter())
        .any(|r| matches!(*r, Region::Param(i) if early.get(i as usize).copied().unwrap_or(true)));
    if early.iter().any(|&slot| slot) {
        let mut signature = Vec::new();
        if !function
            .params
            .iter()
            .all(|p| callback_type_lifetimes(&p.ty, &mut signature))
            || !callback_type_lifetimes(&function.ret, &mut signature)
        {
            return CallbackBinder::Unclassified;
        }
        // Do not infer independence from a missing row when an alias, nested
        // pointer or object syntactically carries the early lifetime.
        bound |= signature.iter().any(|lifetime| {
            regions
                .names
                .iter()
                .rposition(|name| name.name == Some(lifetime.name))
                .is_some_and(|i| early[i])
        });
    }
    if bound {
        CallbackBinder::SignatureBound
    } else {
        if !regions.has_bounds {
            return CallbackBinder::Independent;
        }
        // Independent input binders do not erase type-only conditions such as
        // `T: 'static`. Reconstruct those conditions before certifying a table
        // that reification will later use to prove the function's obligations.
        let names: Vec<_> = regions.names.iter().map(|name| name.name).collect();
        let type_names = crate::generic_names(items, function.owner, &function.generics);
        let mut required = Vec::new();
        let mut errors = Vec::new();
        bounds_outlives(
            items,
            sigs,
            function.loc.module,
            &function.generics,
            &names,
            &type_names,
            crate::self_ty(sigs, function.owner),
            function.owner.and_then(|owner| match owner {
                flux_hir::Owner::Impl(id) => sigs.regions.impls.get(id).map(|r| r.self_ty.as_ref()),
                _ => None,
            }),
            &mut required,
            &mut errors,
        );
        if !errors.is_empty()
            || required.iter().any(|condition| match *condition {
                Outlives::Region(a, b) if a == b => false,
                _ => !regions.outlives.contains(condition),
            })
        {
            return CallbackBinder::Unclassified;
        }
        CallbackBinder::Independent
    }
}

pub(super) fn callback_path_lifetimes(path: &Path, out: &mut Vec<LifetimeRef>) -> bool {
    if let Some(q) = &path.qself
        && (!callback_type_lifetimes(&q.ty, out)
            || q.trait_
                .as_ref()
                .is_some_and(|t| !callback_path_lifetimes(t, out)))
    {
        return false;
    }
    for args in path.args.iter().flatten() {
        let start = out.len();
        out.extend(args.lifetimes.iter().copied());
        if !args.types.iter().all(|t| callback_type_lifetimes(t, out))
            || !args
                .bindings
                .iter()
                .all(|b| callback_type_lifetimes(&b.ty, out))
        {
            return false;
        }
        let mut tail = out.split_off(start);
        tail.retain(|l| !args.bound_lifetimes.contains(&l.name));
        out.extend(tail);
    }
    true
}

pub(super) fn callback_type_lifetimes(ty: &TypeRef, out: &mut Vec<LifetimeRef>) -> bool {
    match ty {
        TypeRef::Ref {
            lifetime, pointee, ..
        } => {
            out.extend(lifetime.iter().copied());
            callback_type_lifetimes(pointee, out)
        }
        TypeRef::Path(p) => callback_path_lifetimes(p, out),
        TypeRef::Tuple(ts) => ts.iter().all(|t| callback_type_lifetimes(t, out)),
        TypeRef::Ptr { pointee, .. } | TypeRef::Slice(pointee) => {
            callback_type_lifetimes(pointee, out)
        }
        TypeRef::Array { elem, len } => {
            callback_type_lifetimes(elem, out) && callback_const_lifetimes(len, out)
        }
        TypeRef::Const(c) => callback_const_lifetimes(c, out),
        TypeRef::FnPtr { params, ret, .. } => {
            params.iter().all(|t| callback_type_lifetimes(t, out))
                && callback_type_lifetimes(ret, out)
        }
        TypeRef::Dyn(d) => {
            out.extend(d.lifetime);
            d.bounds
                .iter()
                .all(|b| callback_path_lifetimes(&b.path, out))
        }
        TypeRef::Never | TypeRef::ImplParam(_) => true,
        // These have no complete predicate mapping at this boundary.
        TypeRef::Infer | TypeRef::Error | TypeRef::ImplTrait(_) | TypeRef::Opaque(_) => false,
    }
}

/// Anonymous constant bodies have no signature lifetime occurrence table.
/// Do not silently certify predicates containing one as independent.
fn callback_const_lifetimes(c: &flux_hir::ArrayLen, out: &mut Vec<LifetimeRef>) -> bool {
    match &c.kind {
        flux_hir::ArrayLenKind::Path(p) => callback_path_lifetimes(p, out),
        flux_hir::ArrayLenKind::Lit { .. }
        | flux_hir::ArrayLenKind::NegLit { .. }
        | flux_hir::ArrayLenKind::Const(_)
        | flux_hir::ArrayLenKind::Bool(_)
        | flux_hir::ArrayLenKind::Char(_) => true,
        flux_hir::ArrayLenKind::Anon(..) | flux_hir::ArrayLenKind::Error => false,
    }
}

/// Erased equality with Self is insufficient: a default `&'static T` can
/// have the same Ty as Self while carrying different regions.
fn trait_default_self(items: &ItemTree, trait_: flux_hir::TraitId, index: usize) -> bool {
    matches!(items.traits[trait_].generics.params.get(index).and_then(|p| p.default.as_ref()),
        Some((TypeRef::Path(path), _)) if path.as_name() == Some(kw::SelfUpper))
}

/// Original trait header predicates and equalities in its owner namespace.
/// No selected implementation requirements are included in these assumptions.
/// Complete source occurrences for a trait header, before selecting only its
/// non-Self predicates for inherited method obligations.
pub struct TraitBoundRegions {
    pub schema: Generics,
    pub predicates: Vec<crate::Predicate>,
    pub subjects: Tables,
    pub traits: Box<[TraitRefRegions]>,
    pub projections: Tables,
    own_indices: Vec<usize>,
}

pub fn trait_bound_regions(
    items: &ItemTree,
    sigs: &Signatures,
    id: flux_hir::TraitId,
) -> Option<(Tables, Box<[TraitRefRegions]>, Tables)> {
    let full = trait_bound_schemes(items, sigs, id)?;
    Some((
        full.own_indices
            .iter()
            .map(|&i| full.subjects[i].clone())
            .collect(),
        full.own_indices
            .iter()
            .map(|&i| full.traits[i].clone())
            .collect(),
        full.projections,
    ))
}

pub fn trait_bound_schemes(
    items: &ItemTree,
    sigs: &Signatures,
    id: flux_hir::TraitId,
) -> Option<TraitBoundRegions> {
    let data = &items.traits[id];
    let names: Vec<_> = sigs
        .regions
        .traits
        .get(id)?
        .names
        .iter()
        .map(|n| n.name)
        .collect();
    let generics = crate::generic_names(
        items,
        Some(flux_hir::Owner::Trait(id)),
        &Generics::default(),
    );
    let signature = sigs.traits.get(id)?;
    let own_predicates = sigs.trait_predicates.get(id).map_or(&[][..], |p| p);
    // Associated-family declaration equalities have their own binders. Only
    // header bindings can be mapped from this header's original syntax.
    let projections = &signature.header_projections;
    let mut errors = Vec::new();
    let self_ty = Ty::param(0, kw::SelfUpper);
    let mut written: Vec<_> = data
        .supertraits
        .iter()
        .cloned()
        .map(|bound| flux_hir::WherePredicate {
            ty: TypeRef::Path(Path::single(kw::SelfUpper)),
            ty_range: bound.range,
            bound,
        })
        .collect();
    let mut predicates: Vec<_> = signature
        .supertraits
        .iter()
        .take(written.len())
        .map(|tr| tr.with_self(self_ty))
        .collect();
    let mut next_super = written.len();
    let mut next_own = 0;
    let mut own_indices = Vec::new();
    let mut bounds = vec![flux_solve::traits::trait_self_predicate(items, id)];
    bounds.extend_from_slice(own_predicates);
    let scope = TypeScope {
        self_ty: Some(self_ty),
        generics: &generics,
        bounds: &bounds,
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        ..TypeScope::default()
    };
    for predicate in &data.generics.predicates {
        let ty = crate::resolve_type(
            items,
            data.loc.module,
            &predicate.ty,
            scope,
            predicate.ty_range,
            &mut errors,
        );
        let resolved = if ty == self_ty {
            let resolved = signature.supertraits.get(next_super)?.with_self(self_ty);
            next_super += 1;
            resolved
        } else {
            let resolved = *own_predicates.get(next_own)?;
            next_own += 1;
            own_indices.push(predicates.len());
            resolved
        };
        predicates.push(resolved);
        written.push(predicate.clone());
    }
    if next_super != signature.supertraits.len() || next_own != own_predicates.len() {
        return None;
    }
    let schema = Generics {
        predicates: written.into(),
        ..Generics::default()
    };
    let rows = bound_regions(
        items,
        data.loc.module,
        &schema,
        &names,
        &generics,
        &predicates,
        projections,
        &mut errors,
    );
    if !errors.is_empty()
        || rows.0.len() != predicates.len()
        || rows.1.len() != predicates.len()
        || rows.2.len() != projections.len()
    {
        return None;
    }
    Some(TraitBoundRegions {
        schema,
        predicates,
        subjects: rows.0,
        traits: rows.1,
        projections: rows.2,
        own_indices,
    })
}

/// The slots of the types `generics`' bounds are on, as `predicates`
/// lists them, and of what they say associated types are, as
/// `projections` does (those that resolved, in order), with the
/// lifetimes `names` and the type parameters `type_names` in scope;
/// none where they cannot be matched, which only errors cause.
#[allow(clippy::too_many_arguments)]
fn bound_regions(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    names: &[Option<Symbol>],
    type_names: &[Symbol],
    predicates: &[crate::Predicate],
    projections: &[crate::traits::Projection],
    errors: &mut Vec<(TextRange, String)>,
) -> (Tables, Box<[TraitRefRegions]>, Tables) {
    let mut walker = Walker::new(items, module, names.to_vec());
    walker.scope = TypeScope {
        generics: type_names,
        ..TypeScope::default()
    };
    walker.add_projection_bounds(generics);
    bound_regions_with_walker(
        items,
        generics,
        predicates,
        projections,
        errors,
        &mut walker,
        None,
    )
}

/// Method bounds use the same selected associated-type rows as their inputs
/// and outputs. Reconstructing a fresh walker here would erase inherited
/// `Self::Item` lifetimes inside callable argument tuples.
fn bound_regions_with_walker(
    items: &ItemTree,
    generics: &Generics,
    predicates: &[crate::Predicate],
    projections: &[crate::traits::Projection],
    errors: &mut Vec<(TextRange, String)>,
    walker: &mut Walker<'_>,
    output_elision: Option<Elided>,
) -> (Tables, Box<[TraitRefRegions]>, Tables) {
    walker.elided = output_elision.unwrap_or(Elided::NotAllowed);
    let written = &generics.predicates;
    let selfs: Tables = if predicates.len() == written.len() {
        written
            .iter()
            .zip(predicates)
            .map(|(w, p)| {
                let saved = enter_bound_binder(walker, &w.bound);
                let row = walker.regions(&w.ty, p.self_ty);
                walker.late_names = saved;
                row
            })
            .collect()
    } else {
        Box::default()
    };
    let traits: Box<[TraitRefRegions]> = if predicates.len() == written.len() {
        written
            .iter()
            .zip(predicates)
            .map(|(w, p)| {
                let bound_scope=enter_bound_binder(walker,&w.bound);
                let last = w.bound.path.segments.len().saturating_sub(1);
                let args = w.bound.path.args(last);
                let mut lifetimes = Vec::new();
                walker.trait_lifetimes(&w.bound.path, p.trait_ref.trait_, false, &mut lifetimes);
                let lifetimes = lifetimes.into();
                let fresh = walker.fresh.len();
                let outer = if args.is_some_and(|a| a.parenthesized) {
                    Some(std::mem::replace(
                        &mut walker.elided,
                        Elided::Fresh,
                    ))
                } else {
                    None
                };
                let late_names = args.filter(|args|args.parenthesized).map(|args| enter_fn_binder(walker, args, p.trait_ref.args.first().copied()));
                // The binder probe may walk anonymous aliases too. The real
                // input walk must start with the same fresh-region namespace.
                walker.fresh.truncate(fresh);
                let mut types: Vec<Box<[Region]>> = p
                    .trait_ref
                    .args
                    .iter()
                    .enumerate()
                    .map(|(i, &ty)| match args.and_then(|a| a.types.get(i)) {
                        Some(t) => walker.regions(t, ty),
                        None if ty == p.self_ty && trait_default_self(items, p.trait_ref.trait_, i) => walker.regions(&w.ty, ty),
                        None => {
                            if slots(items, ty) != 0 {
                                walker.errors.push((w.bound.range, "lifetimes in default trait type arguments are not supported yet".to_owned()));
                            }
                            vec![Region::Erased; slots(items, ty)].into()
                        },
                    })
                    .collect();
                if let Some(outer) = outer {
                    let first_fresh = walker.names.len() + fresh;
                    for row in &mut types {
                        let mut positions = rustc_hash::FxHashMap::default();
                        for (position, region) in row.iter_mut().enumerate() {
                            if let Region::Param(index) = *region
                                && index as usize >= first_fresh {
                                let first = *positions.entry(index).or_insert(position as u32);
                                *region = Region::Late(first);
                            }
                        }
                    }
                    walker.fresh.truncate(fresh);
                    walker.elided = outer;
                }
                if let Some(names) = late_names { walker.late_names = names; }
                walker.late_names=bound_scope;
                TraitRefRegions {
                    bound: w.bound.bound_lifetimes.len() as u32,
                    lifetimes,
                    args: types.into(),
                }
            })
            .collect()
    } else {
        Box::default()
    };
    // Distinct written input domains cannot select a unique erased goal.
    // Compare binders by first occurrence, so alpha-equivalent bounds agree.
    let canonical_inputs = |index: usize| {
        let table = &traits[index];
        let mut bound = rustc_hash::FxHashMap::default();
        selfs[index]
            .iter()
            .chain(table.lifetimes.iter())
            .chain(table.args.iter().flatten())
            .map(|&region| match region {
                Region::Late(i) => {
                    let next = bound.len() as u32;
                    Region::Late(*bound.entry(i).or_insert(next))
                }
                other => other,
            })
            .collect::<Vec<_>>()
    };
    for (index, predicate) in predicates.iter().enumerate().take(traits.len()) {
        if crate::fn_trait_kind(items, predicate.trait_ref.trait_).is_some() {
            continue;
        }
        if (0..index).any(|previous| {
            predicates[previous] == *predicate
                && canonical_inputs(previous) != canonical_inputs(index)
        }) {
            errors.push((
                written[index].bound.range,
                "ambiguous trait lifetime arguments".to_owned(),
            ));
        }
    }
    errors.append(&mut walker.errors);
    let bindings = written.iter().enumerate().flat_map(|(index, w)| {
        let last = w.bound.path.segments.len().saturating_sub(1);
        let args = w.bound.path.args(last);
        args.map_or(&[][..], |a| &a.bindings)
            .iter()
            .map(move |b| (b, args, &w.bound, index))
    });
    let mut tables: Vec<Box<[Region]>> = Vec::with_capacity(projections.len());
    let mut table_owners = Vec::with_capacity(projections.len());
    for ((b, args, bound, index), p) in bindings.zip(projections) {
        if b.name != p.alias.name {
            errors.append(&mut walker.errors);
            return (selfs, traits, Box::default());
        }
        let table = bound_projection_regions(walker, bound, b, args, p);
        if let Some(input) = traits.get(index)
            && input.bound != 0
            && table.iter().any(|region| {
                matches!(region, Region::Late(_))
                    && !selfs[index]
                        .iter()
                        .chain(input.lifetimes.iter())
                        .chain(input.args.iter().flat_map(|row| row.iter()))
                        .any(|r| r == region)
            })
        {
            errors.push((
                b.ty_range,
                "associated type binding references a lifetime absent from trait inputs".into(),
            ));
        }
        let normalize_occurrence = |input: usize, output: &[Region]| {
            let mut binders = rustc_hash::FxHashMap::default();
            selfs[input]
                .iter()
                .chain(traits[input].lifetimes.iter())
                .chain(traits[input].args.iter().flatten())
                .chain(output)
                .map(|&r| match r {
                    Region::Late(i) => {
                        let next = binders.len() as u32;
                        Region::Late(*binders.entry(i).or_insert(next))
                    }
                    r => r,
                })
                .collect::<Vec<_>>()
        };
        // Preserve the source predicate index for every binding, including
        // several bindings on one predicate.
        for ((known, row), &owner) in projections.iter().zip(&tables).zip(&table_owners) {
            if known.alias == p.alias
                && known.ty == p.ty
                && normalize_occurrence(owner, row) != normalize_occurrence(index, &table)
            {
                errors.push((
                    b.ty_range,
                    "ambiguous associated type lifetime arguments".into(),
                ));
            }
        }
        table_owners.push(index);
        tables.push(table);
    }
    if tables.len() != projections.len() {
        errors.append(&mut walker.errors);
        return (selfs, traits, Box::default());
    }
    errors.append(&mut walker.errors);
    (selfs, traits, tables.into())
}

/// A trait argument's requirements must follow from the signature's own
/// bounds and input/self types. Do not turn its required WF into assumptions.
#[allow(clippy::too_many_arguments)]
fn check_trait_wf(
    items: &ItemTree,
    sigs: &Signatures,
    predicate: &crate::Predicate,
    self_regions: &[Region],
    regions: &TraitRefRegions,
    assumptions: &[Outlives<Region>],
    at: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) {
    let Some(requirements) = trait_requirements(items, sigs, predicate, self_regions, regions)
    else {
        errors.push((at, "cannot resolve trait lifetime requirements".to_owned()));
        return;
    };
    for requirement in requirements {
        // A concrete associated type can satisfy the bound without a written
        // assumption. Only normalize types with no region slots: otherwise
        // selection must also reconstruct the output's lifetime mapping.
        let concrete = match requirement {
            Outlives::Type(ty, region) if slots(items, ty) == 0 => {
                let normalized = flux_solve::traits::normalize(items, sigs, &[], ty);
                normalized != ty
                    && slots(items, normalized) == 0
                    && outlives_type_components(sigs, normalized)
                        .into_iter()
                        .all(|component| {
                            outlives_known(Outlives::Type(component, region), assumptions)
                        })
            }
            _ => false,
        };
        if !outlives_known(requirement, assumptions) && !concrete {
            let message = match requirement {
                Outlives::Region(..) => "lifetime bound not satisfied",
                Outlives::Type(..) => "type does not fulfill the required lifetime",
            };
            if !errors.iter().any(|(r, m)| *r == at && m == message) {
                errors.push((at, message.to_owned()));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn check_bound_wf(
    items: &ItemTree,
    sigs: &Signatures,
    generics: &Generics,
    predicates: &[crate::Predicate],
    selfs: &[Box<[Region]>],
    traits: &[TraitRefRegions],
    assumptions: &mut Vec<Outlives<Region>>,
    errors: &mut Vec<(TextRange, String)>,
) {
    // Elaborating a trait bound may assume the trait's bounds on its
    // abstract Self. Bounds on the trait's other arguments remain obligations.
    for (predicate, regions) in predicates.iter().zip(traits) {
        if !matches!(
            predicate.self_ty.kind(),
            TyKind::Param(_) | TyKind::Alias(_)
        ) {
            continue;
        }
        let Some(declaration) = sigs.regions.traits.get(predicate.trait_ref.trait_) else {
            continue;
        };
        for bound in &declaration.outlives {
            if let Outlives::Type(ty, r) = *bound
                && matches!(ty.kind(), TyKind::Param(p) if p.index == 0)
            {
                let r = match r {
                    Region::Param(i) => regions
                        .lifetimes
                        .get(i as usize)
                        .copied()
                        .unwrap_or(Region::Erased),
                    r => r,
                };
                assumptions.push(Outlives::Type(predicate.self_ty, r));
            }
        }
    }
    dedup(assumptions);
    for (((written, predicate), self_regions), regions) in generics
        .predicates
        .iter()
        .zip(predicates)
        .zip(selfs)
        .zip(traits)
    {
        // Parenthesized Fn bounds bind their input regions themselves.
        let last = written.bound.path.segments.len().saturating_sub(1);
        if written
            .bound
            .path
            .args(last)
            .is_some_and(|a| a.parenthesized)
        {
            continue;
        }
        // The quantified subject and trait arguments are well formed inside
        // their binder, just as callback inputs are. Keep assumptions scoped to this
        // occurrence: `for<'a> &'a T: Trait` does not make T globally static.
        let scoped;
        let bound_assumptions = if regions.bound != 0 {
            let mut local = assumptions.clone();
            for (&ty, row) in std::iter::once((&predicate.self_ty, self_regions))
                .chain(predicate.trait_ref.args.iter().zip(regions.args.iter()))
            {
                implied_outlives_with_static(
                    items,
                    sigs,
                    ty,
                    row,
                    Some(Region::Static),
                    &mut local,
                );
            }
            scoped = local;
            &scoped
        } else {
            &*assumptions
        };
        check_trait_wf(
            items,
            sigs,
            predicate,
            self_regions,
            regions,
            bound_assumptions,
            written.bound.range,
            errors,
        );
    }
}

/// Reachability through explicit and implied signature bounds. Erased regions
/// have an earlier resolution diagnostic and cannot cause useful follow-ups.
fn outlives_known(requirement: Outlives<Region>, assumptions: &[Outlives<Region>]) -> bool {
    let reaches = |a, b| {
        let mut seen = vec![a];
        let mut next = 0;
        while next < seen.len() {
            let a = seen[next];
            if a == b || a == Region::Static && b != Region::Static {
                return true;
            }
            for bound in assumptions {
                if let Outlives::Region(x, y) = *bound
                    && x == a
                    && !seen.contains(&y)
                {
                    seen.push(y);
                }
            }
            next += 1;
        }
        false
    };
    match requirement {
        Outlives::Region(a, b) => a == Region::Erased || b == Region::Erased || reaches(a, b),
        Outlives::Type(ty, b) => {
            b == Region::Erased
                || assumptions
                    .iter()
                    .any(|p| matches!(*p, Outlives::Type(t, a) if t == ty && reaches(a, b)))
        }
    }
}

/// A non-callable higher-ranked equality shares its predicate's binder.
fn bound_projection_regions(
    walker: &mut Walker<'_>,
    bound: &flux_hir::Bound,
    binding: &flux_hir::AssocBinding,
    args: Option<&flux_hir::GenericArgs>,
    projection: &crate::traits::Projection,
) -> Box<[Region]> {
    let old = enter_bound_binder(walker, bound);
    let table = projection_regions(walker, binding, args, projection);
    walker.late_names = old;
    table
}

/// A parenthesized `Fn` bound has its own elision scope, including when
/// it occurs in an opaque return type rather than a generic predicate.
fn projection_regions(
    walker: &mut Walker<'_>,
    binding: &flux_hir::AssocBinding,
    args: Option<&flux_hir::GenericArgs>,
    projection: &crate::traits::Projection,
) -> Box<[Region]> {
    let Some(args) = args.filter(|a| a.parenthesized) else {
        return walker.regions(&binding.ty, projection.ty);
    };
    let outer = std::mem::replace(&mut walker.elided, Elided::Is(Region::Erased));
    let late_names = enter_fn_binder(walker, args, projection.alias.args.get(1).copied());
    let mut table = walker.regions(&binding.ty, projection.ty).to_vec();
    if let (Some(input), Some(&input_ty)) = (args.types.first(), projection.alias.args.get(1)) {
        // Count distinct input lifetimes, not slots: `&dyn Trait`
        // stores its one lifetime both on the reference and as the
        // object's default bound.
        let fresh = walker.fresh.len();
        walker.elided = Elided::Fresh;
        let inputs = walker.regions(input, input_ty);
        walker.fresh.truncate(fresh);
        let mut seen = Vec::new();
        for (i, &r) in inputs.iter().enumerate() {
            if !seen.iter().any(|&(region, _)| region == r) {
                let output = match r {
                    Region::Param(j) if j as usize >= walker.names.len() + fresh => {
                        Region::Late(i as u32)
                    }
                    r => r,
                };
                seen.push((r, output));
            }
        }
        if let [(_, only)] = seen[..] {
            for r in &mut table {
                if *r == Region::Erased {
                    *r = only;
                }
            }
        } else if table.contains(&Region::Erased) {
            walker
                .errors
                .push((binding.ty_range, "missing lifetime specifier".to_owned()));
        }
    }
    walker.elided = outer;
    walker.late_names = late_names;
    table.into()
}

fn enter_bound_binder(walker: &mut Walker<'_>, bound: &flux_hir::Bound) -> Vec<(Symbol, u32)> {
    let old = std::mem::take(&mut walker.late_names);
    walker.late_names = bound
        .bound_lifetimes
        .iter()
        .enumerate()
        .map(|(i, &name)| (name, i as u32))
        .collect();
    old
}

/// Map each explicit binder to its first slot in the input tuple. Repeated
/// occurrences then use that same slot, including inside invariant ADTs.
fn enter_fn_binder(
    walker: &mut Walker<'_>,
    args: &flux_hir::GenericArgs,
    input: Option<Ty>,
) -> Vec<(Symbol, u32)> {
    let old = std::mem::take(&mut walker.late_names);
    if args.bound_lifetimes.is_empty() {
        return old;
    }
    walker.late_names = args
        .bound_lifetimes
        .iter()
        .enumerate()
        .map(|(i, &name)| (name, i as u32))
        .collect();
    if let (Some(written), Some(input)) = (args.types.first(), input) {
        let table = walker.regions(written, input);
        for (i, (_, slot)) in walker.late_names.iter_mut().enumerate() {
            *slot = table
                .iter()
                .position(|&r| r == Region::Late(i as u32))
                .map_or(u32::MAX, |j| j as u32);
        }
    }
    old
}

/// The lifetimes of `function`'s opaque types, with `walker` past its
/// parameters (so with the lifetimes elided there) and set up for its
/// return type: those each captures, all in edition 2024 or those appearing
/// in older-edition bounds unless `use<..>` lists them, its `+ 'a` bounds, and what
/// its bounds say associated types are (`Item = &'a T`, with lifetimes
/// elided as in the return type), whose resolved bounds are `sigs`.
fn opaque_regions(
    function: &flux_hir::Function,
    sigs: &[crate::OpaqueSig],
    walker: &mut Walker<'_>,
) -> Vec<OpaqueRegions> {
    let all = walker.names.len() + walker.fresh.len();
    let mut out = Vec::new();
    for (i, decl) in function.opaques.iter().enumerate() {
        let outlives: Box<[Region]> = decl
            .lifetimes
            .iter()
            .map(|&l| walker.named(l))
            .filter(|&r| r != Region::Erased)
            .collect();
        let mut bounds = Vec::new();
        if let Some(sig) = sigs.get(i) {
            if sig.bound_origins.len() != sig.predicates.len() {
                walker
                    .errors
                    .push((decl.range, "invalid opaque bound metadata".into()));
            }
            for (&origin, &predicate) in sig.bound_origins.iter().zip(&sig.predicates) {
                let Some(bound) = decl.bounds.get(origin as usize) else {
                    walker
                        .errors
                        .push((decl.range, "invalid opaque bound origin".into()));
                    continue;
                };
                // Reuse the ordinary bound walker with the exact written
                // occurrence. The synthetic subject is the hidden Self
                // parameter, whose own row comes from the opaque value.
                let generics = Generics {
                    predicates: vec![flux_hir::WherePredicate {
                        ty: TypeRef::Infer,
                        ty_range: bound.range,
                        bound: bound.clone(),
                    }]
                    .into(),
                    ..Generics::default()
                };
                let saved_elided = walker.elided.clone();
                let mut errors = Vec::new();
                let (_, rows, _) = bound_regions_with_walker(
                    walker.items,
                    &generics,
                    &[predicate],
                    &[],
                    &mut errors,
                    walker,
                    Some(saved_elided.clone()),
                );
                walker.elided = saved_elided;
                walker.errors.extend(errors);
                if let Some(regions) = rows.into_vec().pop() {
                    bounds.push(OpaqueBoundRegions {
                        origin,
                        predicate,
                        regions,
                    });
                }
            }
        }
        let mut list = Vec::new();
        let generics = Generics::default();
        check_bounds(
            walker.items,
            walker.module,
            &generics,
            decl.bounds.iter(),
            false,
            true,
            &mut list,
        );
        // Elided lifetimes are allowed in what they say associated types
        // are, as in the return type.
        list.retain(|(_, message)| !message.starts_with("`&` without an explicit lifetime name"));
        walker.errors.append(&mut list);
        let mut projections: Vec<Box<[Region]>> = Vec::new();
        if let Some(sig) = sigs.get(i) {
            let bindings = decl.bounds.iter().enumerate().flat_map(|(origin, b)| {
                let last = b.path.segments.len().saturating_sub(1);
                let args = b.path.args(last);
                args.map_or(&[][..], |a| &a.bindings)
                    .iter()
                    .map(move |binding| (binding, args, b, origin))
            });
            for ((b, args, bound, origin), p) in bindings.zip(sig.projections.iter()) {
                if b.name != p.alias.name {
                    break;
                }
                let table = bound_projection_regions(walker, bound, b, args, p);
                if let Some(input) = bounds.iter().find(|b| b.origin as usize == origin)
                    && input.regions.bound != 0
                    && table.iter().any(|region| {
                        matches!(region, Region::Late(_))
                            && !input
                                .regions
                                .lifetimes
                                .iter()
                                .chain(input.regions.args.iter().flatten())
                                .any(|r| r == region)
                    })
                {
                    walker.errors.push((
                        b.ty_range,
                        "associated type binding references a lifetime absent from trait inputs"
                            .into(),
                    ));
                }
                projections.push(table);
            }
            if projections.len() != sig.projections.len() {
                projections.clear();
            }
        }
        let captures: Box<[Region]> = match &decl.captures {
            None if decl.edition == flux_syntax::Edition::E2024
                || (function.is_async && i + 1 == function.opaques.len()) =>
            {
                (0..all as u32).map(Region::Param).collect()
            }
            None => {
                let mut mentioned = outlives.to_vec();
                mentioned.extend(projections.iter().flatten().copied());
                for bound in &bounds {
                    mentioned.extend(bound.regions.lifetimes.iter().copied());
                    mentioned.extend(bound.regions.args.iter().flatten().copied());
                }
                // Older editions capture lifetimes occurring in these bounds,
                // not unrelated inputs or the method receiver. Type and const
                // parameters remain in OpaqueTy's ordinary generic arguments.
                (0..all as u32)
                    .map(Region::Param)
                    .filter(|r| mentioned.contains(r))
                    .collect()
            }
            Some(list) => {
                let mut captures = Vec::new();
                for capture in list.iter() {
                    let flux_hir::UseCapture::Lifetime(l) = *capture else {
                        continue;
                    };
                    if l.is_static() {
                        walker.errors.push((
                            l.range,
                            format!(
                                "expected lifetime parameter in `use<...>` precise captures list, found `{}`",
                                l.name
                            ),
                        ));
                        continue;
                    }
                    let r = walker.named(l);
                    if r != Region::Erased && !captures.contains(&r) {
                        captures.push(r);
                    }
                }
                captures.into()
            }
        };
        out.push(OpaqueRegions {
            bounds: bounds.into(),
            captures,
            outlives,
            projections: projections.into(),
        });
    }
    out
}

fn dedup<T: PartialEq>(list: &mut Vec<T>) {
    let mut i = 0;
    while i < list.len() {
        if list[..i].contains(&list[i]) {
            list.remove(i);
        } else {
            i += 1;
        }
    }
}

/// Reconstruct the written owner bounds before admission trusts their table.
pub(super) fn trait_outlives_contract(
    items: &ItemTree,
    sigs: &Signatures,
    id: flux_hir::TraitId,
) -> Option<Box<[Outlives<Region>]>> {
    let declaration = items.traits.get(id)?;
    let names: Vec<_> = own_names(&declaration.generics).map(|n| n.name).collect();
    let generics = crate::generic_names(
        items,
        Some(flux_hir::Owner::Trait(id)),
        &Generics::default(),
    );
    let mut out = Vec::new();
    let mut errors = Vec::new();
    bounds_outlives(
        items,
        sigs,
        declaration.loc.module,
        &declaration.generics,
        &names,
        &generics,
        Some(Ty::param(0, kw::SelfUpper)),
        None,
        &mut out,
        &mut errors,
    );
    dedup(&mut out);
    errors.is_empty().then(|| out.into_boxed_slice())
}

/// The lifetime bounds of `generics` (`'a: 'b`, `T: 'a`), with the
/// lifetimes `names` and the type parameters `type_names` in scope,
/// into `out`.
#[allow(clippy::too_many_arguments)]
fn bounds_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    module: ModuleId,
    generics: &Generics,
    names: &[Option<Symbol>],
    type_names: &[Symbol],
    self_ty: Option<Ty>,
    self_regions: Option<&[Region]>,
    out: &mut Vec<Outlives<Region>>,
    errors: &mut Vec<(TextRange, String)>,
) {
    let mut walker = Walker::new(items, module, names.to_vec());
    walker.self_regions = self_regions;
    walker.elided = Elided::NotAllowed;
    for p in generics.outlives.iter() {
        let a = walker.named(p.longer);
        let b = walker.named(p.shorter);
        if a != Region::Erased && b != Region::Erased {
            out.push(Outlives::Region(a, b));
        }
    }
    let scope = TypeScope {
        generics: type_names,
        self_ty,
        ..TypeScope::default()
    };
    // Type-outlives subjects can be associated projections. Resolve them
    // in the declaration's actual bound environment, just as its fields and
    // signature do, so `I::Item: 'a` retains the declaring trait identity.
    let mut scratch = Vec::new();
    let predicates = crate::resolve_predicates(items, sigs, module, generics, scope, &mut scratch);
    let bounds = flux_solve::traits::elaborate(sigs, &predicates.predicates);
    let scope = TypeScope {
        bounds: &bounds,
        ..scope
    };
    walker.scope = scope;
    for p in generics.type_outlives.iter() {
        let r = walker.named(p.lifetime);
        let mut scratch = Vec::new();
        let ty = crate::resolve_type(items, module, &p.ty, scope, p.ty_range, &mut scratch);
        if has_projection_outlives_inputs(items, ty) {
            errors.push((
                p.ty_range,
                "type-outlives bounds on lifetime-bearing projections are not supported yet".into(),
            ));
            continue;
        }
        let regions = walker.regions(&p.ty, ty);
        if r == Region::Erased {
            continue;
        }
        for a in stored_regions(items, ty, &regions) {
            if a != Region::Erased && a != r {
                out.push(Outlives::Region(a, r));
            }
        }
        outlives_params(sigs, ty, r, out);
    }
    errors.append(&mut walker.errors);
}

/// Preserve existing subset guards for type arguments and Fn bounds.
/// Lifetime arity comes from each trait declaration, not its erased types.
/// Quantified supertrait edges have their own region tables. Other untracked
/// associated bounds retain the existing representation guards.
fn check_bounds<'b>(
    items: &ItemTree,
    module: ModuleId,
    generics: &'b Generics,
    more: impl Iterator<Item = &'b flux_hir::Bound>,
    in_core: bool,
    tracked: bool,
    errors: &mut Vec<(TextRange, String)>,
) {
    for bound in generics.predicates.iter().map(|p| &p.bound).chain(more) {
        let tracked = tracked || !bound.bound_lifetimes.is_empty();
        if !tracked
            && bound.path.args.iter().all(Option::is_none)
            && let (Some(id), _) = items.resolve_trait(module, bound)
            && !items.traits[id].generics.lifetimes.is_empty()
        {
            errors.push((
                bound.range,
                "lifetimes in supertraits and associated type bounds are not supported yet"
                    .to_owned(),
            ));
        }
        for args in bound.path.args.iter().flatten() {
            let (trait_id, _) = items.resolve_trait(module, bound);
            let want = trait_id.map_or(0, |id| items.traits[id].generics.lifetimes.len());
            if !tracked && want != 0 {
                errors.push((
                    bound.range,
                    "lifetimes in supertraits and associated type bounds are not supported yet"
                        .to_owned(),
                ));
            }
            // Tracked bounds are resolved, including names and arity, by
            // `bound_regions`. Untracked bounds must not silently erase them.
            if !tracked && let Some(id) = trait_id {
                let names = generics.lifetimes.iter().map(|l| Some(l.name)).collect();
                let mut walker = Walker::new(items, module, names);
                walker.elided = Elided::NotAllowed;
                walker.trait_lifetimes(&bound.path, id, false, &mut Vec::new());
                errors.append(&mut walker.errors);
            }
            let mut named = Vec::new();
            for t in args.types.iter() {
                lifetimes_in(t, &mut named);
            }
            if args.parenthesized {
                if !tracked && let Some(l) = named.iter().find(|l| !l.is_anonymous()) {
                    errors.push((
                        l.range,
                        "named lifetimes in `Fn` bounds are not supported yet".to_owned(),
                    ));
                }
                // An elided lifetime in what it returns is the one its
                // parameters elide, as a function's (E0106 if they elide
                // none or several).
                let mut inputs = Vec::new();
                for t in args.types.iter() {
                    elided_in(t, &mut inputs);
                }
                for b in args.bindings.iter() {
                    let mut found = Vec::new();
                    elided_in(&b.ty, &mut found);
                    lifetimes_in(&b.ty, &mut named);
                    if !tracked
                        && named.iter().any(|l| {
                            !l.is_anonymous()
                                && b.ty_range.start() <= l.range.start()
                                && l.range.end() <= b.ty_range.end()
                        })
                    {
                        errors.push((
                            b.ty_range,
                            "named lifetimes in `Fn` bounds are not supported yet".to_owned(),
                        ));
                    } else if !tracked && !found.is_empty() && inputs.len() != 1 {
                        for at in found {
                            errors.push((at, "missing lifetime specifier".to_owned()));
                        }
                    }
                }
                continue;
            }
            let mut elided = Vec::new();
            for t in args.types.iter() {
                elided_in(t, &mut elided);
            }
            for b in args.bindings.iter() {
                elided_in(&b.ty, &mut elided);
                if !tracked {
                    lifetimes_in(&b.ty, &mut named);
                }
            }
            for at in elided {
                errors.push((
                    at,
                    "`&` without an explicit lifetime name cannot be used here".to_owned(),
                ));
            }
            if let Some(l) = named.iter().find(|l| !l.is_anonymous())
                && !in_core
                && !tracked
            {
                errors.push((
                    l.range,
                    "lifetimes in the generic arguments of bounds are not supported yet".to_owned(),
                ));
            }
        }
    }
}

/// A reference to a selected output must not infer the prerequisites of
/// that selection from the output itself. Check its stored rows against the
/// declaration's existing bounds, independently of captured input rows.
fn selected_pointees_wf(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    row: &[Region],
    bounds: &[Outlives<Region>],
) -> bool {
    if !ty.has_aliases() {
        return true;
    }
    let mut pending = vec![(ty, row)];
    while let Some((ty, row)) = pending.pop() {
        if let TyKind::Ref(_, inner) = ty.kind() {
            let Some((&lifetime, inner_row)) = row.split_first() else {
                return false;
            };
            if let TyKind::Alias(alias) = inner.kind()
                && has_projection_inputs(items, alias)
                && !has_declared_projection_inputs(alias)
            {
                let Some(output) = super::selected::projection(items, sigs, alias, inner_row)
                else {
                    return false;
                };
                let output_ty = flux_solve::traits::normalize(items, sigs, &[], inner);
                if stored_regions(items, output_ty, &output)
                    .into_iter()
                    .any(|r| !outlives_known(Outlives::Region(r, lifetime), bounds))
                    || outlives_type_components(sigs, output_ty)
                        .into_iter()
                        .any(|t| !outlives_known(Outlives::Type(t, lifetime), bounds))
                {
                    return false;
                }
            }
            pending.push((inner, inner_row));
            continue;
        }
        let (children, mut next): (Vec<Ty>, usize) = match ty.kind() {
            TyKind::RawPtr(_, inner) | TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                (vec![inner], 0)
            }
            TyKind::Tuple(list) => (list.to_vec(), 0),
            TyKind::Adt(adt, args) => (
                args.to_vec(),
                items.structs[adt.strukt].generics.lifetimes.len(),
            ),
            TyKind::Dynamic(d) => (
                object_region_types(items, d).collect(),
                object_own_slots(items, d),
            ),
            TyKind::Opaque(o) => (o.args.to_vec(), o.lifetimes as usize),
            _ => continue,
        };
        for child in children {
            let count = slots(items, child);
            let Some(part) = row.get(next..next + count) else {
                return false;
            };
            pending.push((child, part));
            next += count;
        }
    }
    true
}

/// The variances of the parameters of the structs from `from` on, given
/// those before: each the meet of its uses' in fields, until nothing
/// changes, as structs may contain each other.
fn struct_variances(items: &ItemTree, sigs: &mut Signatures, from: usize) {
    loop {
        let mut changed = false;
        for (id, s) in items.structs.iter().skip(from) {
            let lifetimes = s.generics.lifetimes.len();
            let mut acc = sigs.regions.structs[id].variances.to_vec();
            let fields = sigs.structs[id];
            for (&ty, regions) in fields.iter().zip(sigs.regions.structs[id].fields.iter()) {
                let mut walk = VarianceWalk {
                    items,
                    sigs,
                    regions,
                    next: 0,
                    lifetimes,
                    acc: &mut acc,
                };
                walk.go(ty, Variance::Covariant);
            }
            for (alias, regions) in &sigs.regions.structs[id].selected_inputs {
                let mut walk = VarianceWalk {
                    items,
                    sigs,
                    regions,
                    next: 0,
                    lifetimes,
                    acc: &mut acc,
                };
                for &input in alias.args {
                    walk.go(input, Variance::Invariant);
                }
            }
            if *acc != *sigs.regions.structs[id].variances {
                changed = true;
                let mut entry = sigs.regions.structs[id].clone();
                entry.variances = acc.into();
                sigs.regions.structs.insert(id, entry);
            }
        }
        if !changed {
            return;
        }
    }
}

/// The variances of a struct's parameters in one field's type.
struct VarianceWalk<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    regions: &'a [Region],
    next: usize,
    lifetimes: usize,
    acc: &'a mut [Variance],
}

impl VarianceWalk<'_> {
    fn slot(&mut self, v: Variance) {
        if let Some(Region::Param(i)) = self.regions.get(self.next)
            && let Some(a) = self.acc.get_mut(*i as usize)
        {
            *a = a.join(v);
        }
        self.next += 1;
    }

    fn go(&mut self, ty: Ty, ambient: Variance) {
        let of = |m: crate::Mutability| match m {
            crate::Mutability::Not => Variance::Covariant,
            crate::Mutability::Mut => Variance::Invariant,
        };
        match ty.kind() {
            TyKind::Ref(m, inner) => {
                self.slot(ambient);
                self.go(inner, ambient.xform(of(m)));
            }
            TyKind::RawPtr(m, inner) => self.go(inner, ambient.xform(of(m))),
            TyKind::Array(t, _) | TyKind::Slice(t) => self.go(t, ambient),
            TyKind::Tuple(list) => {
                for &t in list {
                    self.go(t, ambient);
                }
            }
            TyKind::Adt(adt, args) => {
                let other = self.sigs.regions.structs.get(adt.strukt);
                let n = self.items.structs[adt.strukt].generics.lifetimes.len();
                let variance = |k: usize| {
                    other
                        .and_then(|o| o.variances.get(k).copied())
                        .unwrap_or(Variance::Invariant)
                };
                for k in 0..n {
                    self.slot(ambient.xform(variance(k)));
                }
                for (j, &t) in args.iter().enumerate() {
                    self.go(t, ambient.xform(variance(n + j)));
                }
            }
            TyKind::Param(p) => {
                if let Some(a) = self.acc.get_mut(self.lifetimes + p.index as usize) {
                    *a = a.join(ambient);
                }
            }
            TyKind::FnPtr(sig) => {
                if let Some(r) = sig.regions {
                    let (free, types) = super::fn_pointer_variances(self.items, self.sigs, sig)
                        .unwrap_or_else(|| {
                            (
                                vec![Variance::Invariant; r.free as usize],
                                vec![Variance::Invariant; r.types.len()],
                            )
                        });
                    for variance in free {
                        self.slot(ambient.xform(variance));
                    }
                    for (&ty, variance) in r.types.iter().zip(types) {
                        self.go(ty, ambient.xform(variance));
                    }
                }
            }
            TyKind::Alias(alias) => {
                let invariant = ambient.xform(Variance::Invariant);
                for _ in 0..alias.owner_lifetimes {
                    self.slot(invariant);
                }
                for &t in alias.args {
                    self.go(t, invariant);
                }
                for _ in 0..alias.family_lifetimes {
                    self.slot(invariant);
                }
                for &t in alias.family_args {
                    self.go(t, invariant);
                }
            }
            // `dyn Tr + 'a` is covariant in `'a`.
            TyKind::Dynamic(object) => {
                self.slot(ambient);
                for _ in 1..super::object_own_slots(self.items, object) {
                    self.slot(ambient.xform(Variance::Invariant));
                }
                for ty in object_region_types(self.items, object) {
                    self.go(ty, ambient.xform(Variance::Invariant));
                }
            }
            _ => {}
        }
    }
}

/// What the parameters of the structs from `from` on outlive: their
/// bounds, and what their fields' types imply, until nothing changes.
fn struct_outlives(items: &ItemTree, sigs: &mut Signatures, from: usize, errors: &mut Errors) {
    let mut explicit: Vec<Vec<Outlives<Region>>> = Vec::new();
    for (id, s) in items.structs.iter().skip(from) {
        let names: Vec<Option<Symbol>> =
            s.generics.lifetimes.iter().map(|l| Some(l.name)).collect();
        let type_names: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let mut out = Vec::new();
        let mut list = Vec::new();
        let self_regions: Vec<_> = (0..s.generics.lifetimes.len() as u32)
            .map(Region::Param)
            .collect();
        bounds_outlives(
            items,
            sigs,
            s.loc.module,
            &s.generics,
            &names,
            &type_names,
            Some(Ty::adt(id, s.name, &crate::params_of(&s.generics, 0))),
            Some(&self_regions),
            &mut out,
            &mut list,
        );
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (s.loc.file, r, m)));
        if let Some(metadata) = sigs.regions.structs.get(id) {
            check_bound_wf(
                items,
                sigs,
                &s.generics,
                sigs.struct_predicates.get(id).map_or(&[][..], |p| p),
                &metadata.bound_selfs,
                &metadata.bound_traits,
                &mut out,
                &mut Vec::new(),
            );
        }
        explicit.push(out);
    }
    // Each round adds what one more level of nesting implies.
    for _ in 0..16 {
        let mut changed = false;
        for (k, (id, _)) in items.structs.iter().skip(from).enumerate() {
            let mut out = explicit[k].clone();
            let fields = sigs.structs[id];
            let field_regions = sigs.regions.structs[id].fields.clone();
            for (&ty, regions) in fields.iter().zip(field_regions.iter()) {
                implied_outlives_with_static(
                    items,
                    sigs,
                    ty,
                    regions,
                    Some(Region::Static),
                    &mut out,
                );
            }
            for (alias, regions) in &sigs.regions.structs[id].selected_inputs {
                let mut at = 0;
                for &input in alias.args {
                    let count = slots(items, input);
                    implied_outlives_with_static(
                        items,
                        sigs,
                        input,
                        &regions[at..at + count],
                        Some(Region::Static),
                        &mut out,
                    );
                    at += count;
                }
            }
            out.retain(|p| match *p {
                Outlives::Region(a, b) => {
                    a != Region::Erased && b != Region::Erased && a != b && a != Region::Static
                }
                Outlives::Type(_, b) => b != Region::Erased,
            });
            dedup(&mut out);
            if out.len() != sigs.regions.structs[id].outlives.len() {
                changed = true;
                let mut entry = sigs.regions.structs[id].clone();
                entry.outlives = out.into();
                sigs.regions.structs.insert(id, entry);
            }
        }
        if !changed {
            break;
        }
    }
    // Static formation requirements are explicit declaration obligations.
    // Ordinary reference outlives inference must not silently turn an invalid
    // `struct S<'a> { value: Box<dyn StaticTrait + 'a> }` into a bounded ADT.
    for (k, (id, data)) in items.structs.iter().skip(from).enumerate() {
        let metadata = &sigs.regions.structs[id];
        for ((field, &ty), row) in crate::all_fields(data)
            .zip(sigs.structs[id].iter())
            .zip(metadata.fields.iter())
        {
            if !selected_pointees_wf(items, sigs, ty, row, &metadata.outlives) {
                errors.push((
                    data.loc.file,
                    field.range,
                    "selected projection output lifetime bound is not satisfied".into(),
                ));
            }
            let mut required = Vec::new();
            implied_outlives_with_static(items, sigs, ty, row, Some(Region::Static), &mut required);
            if required.into_iter().any(|bound| {
                matches!(bound, Outlives::Region(_, Region::Static))
                    && !outlives_known(bound, &explicit[k])
            }) {
                errors.push((
                    data.loc.file,
                    field.range,
                    "lifetime bound not satisfied".to_owned(),
                ));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../flux-syntax/tests/support/scaling.rs"]
mod static_bound_scaling;

#[cfg(test)]
mod static_bound_tests {
    use super::*;

    #[test]
    fn static_self_propagation_preserves_extension_boundary() {
        let source = "trait P:'static{} trait C:P{} trait G:C{} fn main(){}";
        let (_, _, hir) = flux_hir::lower_text("static-traits.rs", source);
        let (mut sigs, errors) = crate::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let find = |name: &str| {
            hir.items
                .traits
                .iter()
                .find(|(_, data)| data.name.as_str() == name)
                .unwrap()
                .0
        };
        let child = find("C");
        let grandchild = find("G");
        sigs.regions.traits.get_mut(child).unwrap().outlives = Box::default();
        sigs.regions.traits.get_mut(grandchild).unwrap().outlives = Box::default();
        let mut from = hir.items.core;
        from.traits = grandchild.index() as u32;
        inherit_static_self(&hir.items, &mut sigs, from);
        assert!(sigs.regions.traits[child].outlives.is_empty());
        assert_eq!(
            sigs.regions.traits[grandchild].outlives.as_ref(),
            [Outlives::Type(
                flux_solve::traits::trait_self_predicate(&hir.items, grandchild).self_ty,
                Region::Static
            )]
        );
    }

    /// No trait in this chain declares a static bound. Searching every
    /// ancestor from every declaration must not make this common case cubic.
    #[test]
    fn static_self_trait_chain_scales() {
        static_bound_scaling::assert_scales(
            "static Self propagation through trait chains",
            128,
            |count| {
                let mut source = String::from("trait T0 {}\n");
                for index in 1..count {
                    source.push_str(&format!("trait T{index}: T{} {{}}\n", index - 1));
                }
                source.push_str("fn main() {}\n");
                let (_, _, hir) = flux_hir::lower_text("static-traits.rs", &source);
                assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
                let (sigs, errors) = crate::signatures(&hir.items);
                assert!(errors.is_empty(), "{errors:?}");
                (hir.items, std::cell::RefCell::new(sigs))
            },
            |(items, sigs)| {
                // Amortize timer noise without timing parsing/signature work.
                for _ in 0..512 {
                    inherit_static_self(items, &mut sigs.borrow_mut(), items.core);
                }
            },
        );
    }
}

#[cfg(test)]
mod callback_metadata_tests {
    use super::*;

    #[test]
    fn independent_callback_retains_static_type_obligations() {
        let source = "fn get<T:'static>(x:&T)->&T{x}fn main(){}";
        let (_, _, hir) = flux_hir::lower_text("static-metadata.rs", source);
        let (sigs, errors) = crate::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let id = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "get" && f.owner.is_none())
            .unwrap()
            .0;
        let mut row = sigs.regions.fns[id].clone();
        assert_eq!(row.callback_binder, CallbackBinder::Independent);
        row.outlives = Box::default();
        assert_eq!(
            callback_binder(&hir.items, &sigs, id, &row),
            CallbackBinder::Unclassified
        );
    }

    #[test]
    fn incomplete_predicate_classification_fails_closed() {
        let source = "trait Tr<T>{type Out;} impl<T> Tr<T> for (){type Out=T;} fn get<'a>(_:&'a i32) where ():Tr<&'a i32,Out=&'a i32>{} fn main(){}";
        let (_, _, hir) = flux_hir::lower_text("metadata.rs", source);
        let (sigs, errors) = crate::signatures(&hir.items);
        assert!(
            errors.iter().all(|d| d.message
                == "lifetimes in the generic arguments of bounds are not supported yet"),
            "{errors:?}"
        );
        let id = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "get" && f.owner.is_none())
            .unwrap()
            .0;
        let original = &sigs.regions.fns[id];
        assert_eq!(original.callback_binder, CallbackBinder::SignatureBound);
        for corruption in 0..8 {
            let mut r = original.clone();
            match corruption {
                0 => r.bound_selfs = Box::default(),
                1 => r.bound_traits = Box::default(),
                2 => r.bound_traits[0].args = Box::default(),
                3 => r.bound_traits[0].args[0] = Box::default(),
                4 => r.bound_traits[0].args[0][0] = Region::Param(999),
                5 => r.projections = Box::default(),
                6 => r.projections[0] = Box::default(),
                _ => r.owner = 1,
            }
            assert_eq!(
                callback_binder(&hir.items, &sigs, id, &r),
                CallbackBinder::Unclassified,
                "{corruption}: {r:?}"
            );
        }
    }
}
