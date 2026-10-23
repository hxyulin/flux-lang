//! Resolving the lifetimes of every signature, and the checks on them.

use super::*;

type Errors = Vec<(flux_hir::FileId, TextRange, String)>;
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
    // Retain trait owner regions even while lifetime declarations are rejected.
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let mut list = Vec::new();
        for l in t.generics.lifetimes.iter() {
            list.push((
                l.range,
                "lifetime parameters on traits are not supported yet".to_owned(),
            ));
        }
        check_bounds(
            items,
            t.loc.module,
            &t.generics,
            t.supertraits.iter(),
            items.is_core(t.loc.module),
            false,
            &mut list,
        );
        for ty in &t.types {
            check_bounds(
                items,
                t.loc.module,
                &Generics::default(),
                ty.bounds.iter(),
                items.is_core(t.loc.module),
                false,
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
            t.loc.module,
            &t.generics,
            &named,
            &generics,
            &mut outlives,
            &mut list,
        );
        dedup(&mut outlives);
        sigs.regions.traits.insert(
            id,
            TraitRegions {
                names,
                outlives: outlives.into(),
            },
        );
        push(&mut errors, t.loc.file, list);
    }
    structs(items, sigs, from, &mut errors);
    impls(items, sigs, from, &mut errors);
    functions(items, sigs, from, &mut errors);
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
            false,
            &mut list,
        );
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (s.loc.file, r, m)));
        let count = s.generics.lifetimes.len() + s.generics.params.len();
        sigs.regions.structs.insert(
            id,
            StructRegions {
                lifetimes: s.generics.lifetimes.len() as u32,
                fields,
                // Mutation through a shared reference makes the payload
                // invariant, despite storing `T` directly in these core types.
                variances: if crate::is_lang_struct(items, id, "unsafe_cell")
                    || crate::is_lang_struct(items, id, "cell")
                    || crate::is_lang_struct(items, id, "ref_cell")
                {
                    vec![Variance::Invariant; count].into()
                } else if crate::is_lang_struct(items, id, "phantom_data") {
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
    for (id, s) in items.structs.iter().skip(from) {
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
        walker.elided = Elided::Fresh;
        let self_regions = walker.regions(&imp.self_ty, self_ty);
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
                let regions = match written
                    .filter(|w| !w.parenthesized)
                    .and_then(|w| w.types.get(i))
                {
                    Some(t) => walker.regions(t, arg),
                    None if arg == self_ty && trait_default_self(items, trait_ref.trait_, i) => {
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
        walker.elided = Elided::Missing(None);
        let mut type_selections = Vec::new();
        let types: Box<[(Symbol, Box<[Region]>)]> = imp
            .types
            .iter()
            .map(|t| {
                let ty = crate::traits::impl_type(sigs, id, t.name).unwrap_or(Ty::ERROR);
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
            imp.module,
            &imp.generics,
            &names,
            &generics,
            &mut outlives,
            &mut list,
        );
        dedup(&mut outlives);
        let bounds: Box<[Outlives<Region>]> = outlives.clone().into();
        implied_outlives(items, sigs, self_ty, &self_regions, &mut outlives);
        // As with the self type, Rust assumes the header's argument types
        // are well formed. The trait declaration's explicit bounds still
        // need proof, e.g. `impl Tr<'a, &'b T>` cannot assume `'b: 'a`.
        if let Some(trait_ref) = trait_ref {
            for (&arg, regions) in trait_ref.args.iter().zip(&trait_args) {
                implied_outlives(items, sigs, arg, regions, &mut outlives);
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
                names: all_names.into(),
                self_ty: self_regions,
                trait_args: trait_args.into(),
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
    let (imp, args) = crate::traits::select_impl(items, sigs, goal)?;
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
    let ty = crate::traits::impl_type(sigs, imp, name)?;
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
            let candidates = crate::traits::associated_self_bounds(
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
        let self_ty = crate::self_ty(sigs, function.owner);
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
        walker.elided = Elided::Fresh;
        let (sig_params, mut sig_ret) = (sigs.fns[id].params.clone(), sigs.fns[id].ret);
        let params: Box<[Box<[Region]>]> = function
            .params
            .iter()
            .zip(sig_params.iter())
            .map(|(p, &ty)| walker.regions(&p.ty, ty))
            .collect();
        // Elision in the return type.
        let first_fresh = walker.names.len() as u32;
        let self_ref = function.has_self
            && matches!(
                function.params.first().map(|p| &p.ty),
                Some(TypeRef::Ref { .. })
            );
        let inputs: Vec<Region> = params.iter().flat_map(|p| p.iter().copied()).collect();
        // Each parameter's lifetimes, each once, as rustc counts them:
        // `&'a S<'a>`, and `&dyn Tr`, whose lifetime bound is its
        // reference's, have one; `x: &'a i32, y: &'a i32` two.
        let mut distinct: Vec<Region> = Vec::new();
        for p in params.iter() {
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
        let ret = walker.regions(&function.ret, sig_ret);
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
            function.loc.module,
            &function.generics,
            &names,
            &generics,
            &mut outlives,
            &mut list,
        );
        for (&ty, regions) in sig_params.iter().zip(params.iter()) {
            implied_outlives(items, sigs, ty, regions, &mut outlives);
        }
        implied_outlives(items, sigs, sig_ret, &ret, &mut outlives);
        outlives.retain(|p| match *p {
            Outlives::Region(a, b) => a != Region::Erased && b != Region::Erased && a != b,
            Outlives::Type(_, b) => b != Region::Erased,
        });
        dedup(&mut outlives);
        let (bound_selfs, bound_traits, projections) = bound_regions(
            items,
            function.loc.module,
            &function.generics,
            &names,
            &generics,
            sigs.fn_predicates.get(id).map_or(&[][..], |p| p),
            sigs.fn_projections.get(id).map_or(&[][..], |p| p),
            &mut list,
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
    let mut bound = regions.has_bounds;
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
        bound |= is_own;
    }
    let mut written = Vec::new();
    for p in &function.generics.predicates {
        if !callback_type_lifetimes(&p.ty, &mut written)
            || !callback_path_lifetimes(&p.bound.path, &mut written)
        {
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
        bound |= i >= regions.owner as usize;
    }
    if bound {
        CallbackBinder::SignatureBound
    } else {
        CallbackBinder::Independent
    }
}

fn callback_path_lifetimes(path: &Path, out: &mut Vec<LifetimeRef>) -> bool {
    if let Some(q) = &path.qself
        && (!callback_type_lifetimes(&q.ty, out)
            || q.trait_
                .as_ref()
                .is_some_and(|t| !callback_path_lifetimes(t, out)))
    {
        return false;
    }
    for args in path.args.iter().flatten() {
        out.extend(args.lifetimes.iter().copied());
        if !args.types.iter().all(|t| callback_type_lifetimes(t, out))
            || !args
                .bindings
                .iter()
                .all(|b| callback_type_lifetimes(&b.ty, out))
        {
            return false;
        }
    }
    true
}

fn callback_type_lifetimes(ty: &TypeRef, out: &mut Vec<LifetimeRef>) -> bool {
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
    walker.elided = Elided::NotAllowed;
    let written = &generics.predicates;
    let selfs = if predicates.len() == written.len() {
        written
            .iter()
            .zip(predicates)
            .map(|(w, p)| walker.regions(&w.ty, p.self_ty))
            .collect()
    } else {
        Box::default()
    };
    let traits = if predicates.len() == written.len() {
        written
            .iter()
            .zip(predicates)
            .map(|(w, p)| {
                let last = w.bound.path.segments.len().saturating_sub(1);
                let args = w.bound.path.args(last);
                let mut lifetimes = Vec::new();
                walker.trait_lifetimes(&w.bound.path, p.trait_ref.trait_, false, &mut lifetimes);
                let lifetimes = lifetimes.into();
                let outer = if args.is_some_and(|a| a.parenthesized) {
                    Some(std::mem::replace(
                        &mut walker.elided,
                        Elided::Is(Region::Erased),
                    ))
                } else {
                    None
                };
                let types = p
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
                    walker.elided = outer;
                }
                TraitRefRegions {
                    lifetimes,
                    args: types,
                }
            })
            .collect()
    } else {
        Box::default()
    };
    errors.append(&mut walker.errors);
    let bindings = written.iter().flat_map(|w| {
        let last = w.bound.path.segments.len().saturating_sub(1);
        let args = w.bound.path.args(last);
        args.map_or(&[][..], |a| &a.bindings)
            .iter()
            .map(move |b| (b, args))
    });
    let mut tables = Vec::with_capacity(projections.len());
    for ((b, args), p) in bindings.zip(projections) {
        if b.name != p.alias.name {
            errors.append(&mut walker.errors);
            return (selfs, traits, Box::default());
        }
        tables.push(projection_regions(&mut walker, b, args, p));
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
        if !outlives_known(requirement, assumptions) {
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
        if !matches!(predicate.self_ty.kind(), TyKind::Param(_)) {
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
        check_trait_wf(
            items,
            sigs,
            predicate,
            self_regions,
            regions,
            assumptions,
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
    let mut table = walker.regions(&binding.ty, projection.ty).to_vec();
    if let (Some(input), Some(&input_ty)) = (args.types.first(), projection.alias.args.get(1)) {
        // Count distinct input lifetimes, not slots: `&dyn Trait`
        // stores its one lifetime both on the reference and as the
        // object's default bound.
        let fresh = walker.fresh.len();
        walker.elided = Elided::Fresh;
        let inputs = walker.regions(input, input_ty);
        walker.fresh.truncate(fresh);
        let mut elided = Vec::new();
        let mut seen = Vec::new();
        for (i, &r) in inputs.iter().enumerate() {
            if matches!(r, Region::Param(j) if j as usize >= walker.names.len() + fresh)
                && !seen.contains(&r)
            {
                seen.push(r);
                elided.push(i);
            }
        }
        if let [only] = elided[..] {
            for r in &mut table {
                if *r == Region::Erased {
                    *r = Region::Late(only as u32);
                }
            }
        }
    }
    walker.elided = outer;
    table.into()
}

/// The lifetimes of `function`'s opaque types, with `walker` past its
/// parameters (so with the lifetimes elided there) and set up for its
/// return type: those each captures, all of the function's unless
/// `use<..>` lists them, those its `+ 'a` bounds name, and those of what
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
        let captures: Box<[Region]> = match &decl.captures {
            None => (0..all as u32).map(Region::Param).collect(),
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
        let outlives: Box<[Region]> = decl
            .lifetimes
            .iter()
            .map(|&l| walker.named(l))
            .filter(|&r| r != Region::Erased)
            .collect();
        for bound in &decl.bounds {
            let (trait_, _) = walker.items.resolve_trait(walker.module, bound);
            if trait_.is_some_and(|id| !walker.items.traits[id].generics.lifetimes.is_empty())
                || bound
                    .path
                    .args
                    .iter()
                    .flatten()
                    .any(|a| !a.lifetimes.is_empty())
            {
                walker.errors.push((
                    bound.range,
                    "lifetimes in opaque trait arguments are not supported yet".to_owned(),
                ));
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
            let bindings = decl.bounds.iter().flat_map(|b| {
                let last = b.path.segments.len().saturating_sub(1);
                let args = b.path.args(last);
                args.map_or(&[][..], |a| &a.bindings)
                    .iter()
                    .map(move |b| (b, args))
            });
            for ((b, args), p) in bindings.zip(sig.projections.iter()) {
                if b.name != p.alias.name {
                    break;
                }
                projections.push(projection_regions(walker, b, args, p));
            }
            if projections.len() != sig.projections.len() {
                projections.clear();
            }
        }
        out.push(OpaqueRegions {
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

/// The lifetime bounds of `generics` (`'a: 'b`, `T: 'a`), with the
/// lifetimes `names` and the type parameters `type_names` in scope,
/// into `out`.
fn bounds_outlives(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    names: &[Option<Symbol>],
    type_names: &[Symbol],
    out: &mut Vec<Outlives<Region>>,
    errors: &mut Vec<(TextRange, String)>,
) {
    let mut walker = Walker::new(items, module, names.to_vec());
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
        // A trait's first type parameter is `Self`; unlike an impl's,
        // it is an abstract parameter rather than a concrete header type.
        self_ty: type_names
            .iter()
            .position(|&n| n == kw::SelfUpper)
            .map(|i| Ty::param(i as u32, kw::SelfUpper)),
        ..TypeScope::default()
    };
    walker.scope = scope;
    for p in generics.type_outlives.iter() {
        let r = walker.named(p.lifetime);
        let mut scratch = Vec::new();
        let ty = crate::resolve_type(items, module, &p.ty, scope, p.ty_range, &mut scratch);
        let regions = walker.regions(&p.ty, ty);
        if r == Region::Erased {
            continue;
        }
        for &a in regions.iter() {
            if a != Region::Erased && a != r {
                out.push(Outlives::Region(a, r));
            }
        }
        outlives_params(ty, r, out);
    }
    errors.append(&mut walker.errors);
}

/// Preserve existing subset guards for type arguments and Fn bounds.
/// Lifetime arity comes from each trait declaration, not its erased types.
/// Supertraits and associated type bounds have no region tables yet.
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
                if let Some(l) = named.iter().find(|l| !l.is_anonymous()) {
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
                    if named.iter().any(|l| {
                        !l.is_anonymous()
                            && b.ty_range.start() <= l.range.start()
                            && l.range.end() <= b.ty_range.end()
                    }) {
                        errors.push((
                            b.ty_range,
                            "named lifetimes in `Fn` bounds are not supported yet".to_owned(),
                        ));
                    } else if !found.is_empty() && inputs.len() != 1 {
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
            {
                errors.push((
                    l.range,
                    "lifetimes in the generic arguments of bounds are not supported yet".to_owned(),
                ));
            }
        }
    }
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
            // Types without slots, whose parameters still count.
            TyKind::FnPtr(sig) => {
                for &t in sig.params() {
                    self.go(t, ambient.xform(Variance::Contravariant));
                }
                self.go(sig.ret(), ambient);
            }
            TyKind::Alias(alias) => {
                for &t in alias.args {
                    self.go(t, ambient.xform(Variance::Invariant));
                }
            }
            // `dyn Tr + 'a` is covariant in `'a`.
            TyKind::Dynamic(_) => self.slot(ambient),
            _ => {}
        }
    }
}

/// What the parameters of the structs from `from` on outlive: their
/// bounds, and what their fields' types imply, until nothing changes.
fn struct_outlives(items: &ItemTree, sigs: &mut Signatures, from: usize, errors: &mut Errors) {
    let mut explicit: Vec<Vec<Outlives<Region>>> = Vec::new();
    for (_, s) in items.structs.iter().skip(from) {
        let names: Vec<Option<Symbol>> =
            s.generics.lifetimes.iter().map(|l| Some(l.name)).collect();
        let type_names: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let mut out = Vec::new();
        let mut list = Vec::new();
        bounds_outlives(
            items,
            s.loc.module,
            &s.generics,
            &names,
            &type_names,
            &mut out,
            &mut list,
        );
        dedup(&mut list);
        errors.extend(list.into_iter().map(|(r, m)| (s.loc.file, r, m)));
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
}

#[cfg(test)]
mod callback_metadata_tests {
    use super::*;

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
