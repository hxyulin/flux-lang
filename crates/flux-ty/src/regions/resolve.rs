//! Resolving the lifetimes of every signature, and the checks on them.

use super::*;

type Errors = Vec<(flux_hir::FileId, TextRange, String)>;

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
    let push = |errors: &mut Errors, file, list: Vec<(TextRange, String)>| {
        errors.extend(list.into_iter().map(|(r, m)| (file, r, m)));
    };
    // Traits: lifetime parameters are not supported.
    for (_, t) in items.traits.iter().skip(from.traits as usize) {
        let mut list = Vec::new();
        for l in t.generics.lifetimes.iter() {
            list.push((
                l.range,
                "lifetime parameters on traits are not supported yet".to_owned(),
            ));
        }
        check_bounds(&t.generics, t.supertraits.iter(), &mut list);
        for ty in &t.types {
            check_bounds(&Generics::default(), ty.bounds.iter(), &mut list);
        }
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
        check_bounds(&s.generics, std::iter::empty(), &mut list);
        errors.extend(list.into_iter().map(|(r, m)| (s.loc.file, r, m)));
        let count = s.generics.lifetimes.len() + s.generics.params.len();
        sigs.regions.structs.insert(
            id,
            StructRegions {
                lifetimes: s.generics.lifetimes.len() as u32,
                fields,
                variances: vec![Variance::Bivariant; count].into(),
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
        if let (Some(trait_ref), Some(bound)) = (trait_ref, &imp.trait_ref) {
            let last = bound.path.segments.len().saturating_sub(1);
            let written = bound.path.args(last);
            if let Some(w) = written
                && let Some(&l) = w.lifetimes.first()
            {
                walker
                    .errors
                    .push((l.range, no_lifetimes("trait", w.lifetimes.len())));
            }
            for (i, &arg) in trait_ref.args.iter().enumerate() {
                let regions = match written
                    .filter(|w| !w.parenthesized)
                    .and_then(|w| w.types.get(i))
                {
                    Some(t) => walker.regions(t, arg),
                    None if arg == self_ty => self_regions.clone(),
                    None => vec![Region::Erased; slots(items, arg)].into(),
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
        let types: Box<[(Symbol, Box<[Region]>)]> = imp
            .types
            .iter()
            .map(|t| {
                let ty = crate::traits::impl_type(sigs, id, t.name).unwrap_or(Ty::ERROR);
                (t.name, walker.regions(&t.ty, ty))
            })
            .collect();
        let mut list = std::mem::take(&mut walker.errors);
        let names = walker.names.clone();
        check_bounds(&imp.generics, std::iter::empty(), &mut list);
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
        implied_outlives(items, sigs, self_ty, &self_regions, &mut outlives);
        if let Some(trait_ref) = trait_ref {
            for (&arg, regions) in trait_ref.args.iter().zip(&trait_args) {
                implied_outlives(items, sigs, arg, regions, &mut outlives);
            }
        }
        // A trait's block is chosen by its types alone, so its header
        // must hold whatever their lifetimes are: each a parameter of its
        // own, used once, without bounds.
        if imp.trait_ref.is_some() {
            let mut seen = Vec::new();
            let general = self_regions
                .iter()
                .chain(trait_args.iter().flat_map(|a| a.iter()))
                .all(|r| match r {
                    Region::Param(i) if !seen.contains(i) => {
                        seen.push(*i);
                        true
                    }
                    Region::Erased => true,
                    _ => false,
                })
                && imp.generics.outlives.is_empty()
                && imp.generics.type_outlives.is_empty();
            if !general {
                list.push((
                    imp.def_range,
                    "trait implementations for some lifetimes only are not supported yet"
                        .to_owned(),
                ));
            }
        }
        errors.extend(list.into_iter().map(|(r, m)| (imp.file, r, m)));
        dedup(&mut outlives);
        sigs.regions.impls.insert(
            id,
            ImplRegions {
                names: all_names.into(),
                self_ty: self_regions,
                trait_args: trait_args.into(),
                types,
                outlives: outlives.into(),
            },
        );
    }
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
        if let Some(flux_hir::Owner::Impl(imp)) = function.owner
            && let Some(r) = sigs.regions.impls.get(imp)
        {
            owner_names = r.names.to_vec();
            owner_outlives = r.outlives.to_vec();
            self_regions = Some(r.self_ty.clone());
        }
        let owner_count = owner_names.len() as u32;
        let mut names: Vec<Option<Symbol>> = owner_names.iter().map(|n| n.name).collect();
        names.extend(function.generics.lifetimes.iter().map(|l| Some(l.name)));
        let generics = crate::generic_names(items, function.owner, &function.generics);
        let self_ty = crate::self_ty(sigs, function.owner);
        let mut walker = Walker::new(items, function.loc.module, names);
        walker.self_regions = self_regions.as_deref();
        walker.scope = TypeScope {
            self_ty,
            generics: &generics,
            signature_of: Some("functions"),
            ..TypeScope::default()
        };
        walker.elided = Elided::Fresh;
        let sig = &sigs.fns[id];
        let params: Box<[Box<[Region]>]> = function
            .params
            .iter()
            .zip(sig.params.iter())
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
        walker.elided = match (self_ref, params.first()) {
            (true, Some(first)) if !first.is_empty() => Elided::Is(first[0]),
            _ if inputs.len() == 1 => Elided::Is(inputs[0]),
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
        let ret = walker.regions(&function.ret, sig.ret);
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
        check_bounds(&function.generics, std::iter::empty(), &mut list);
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
        let (sig_params, sig_ret) = (sig.params.clone(), sig.ret);
        for (&ty, regions) in sig_params.iter().zip(params.iter()) {
            implied_outlives(items, sigs, ty, regions, &mut outlives);
        }
        implied_outlives(items, sigs, sig_ret, &ret, &mut outlives);
        outlives.retain(|p| match *p {
            Outlives::Region(a, b) => a != Region::Erased && b != Region::Erased && a != b,
            Outlives::Type(_, b) => b != Region::Erased,
        });
        dedup(&mut outlives);
        errors.extend(list.into_iter().map(|(r, m)| (function.loc.file, r, m)));
        sigs.regions.fns.insert(
            id,
            FnRegions {
                names: all_names.into(),
                owner: owner_count,
                params,
                ret,
                outlives: outlives.into(),
                has_bounds: !function.generics.outlives.is_empty()
                    || !function.generics.type_outlives.is_empty(),
            },
        );
    }
}

/// rustc's E0107 for lifetime arguments to a `kind` that takes none.
fn no_lifetimes(kind: &str, n: usize) -> String {
    format!(
        "{kind} takes 0 lifetime arguments but {n} lifetime argument{} supplied",
        if n == 1 { " was" } else { "s were" }
    )
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
        ..TypeScope::default()
    };
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

/// Reports lifetimes in the generic arguments of `generics`' trait
/// bounds and of `more`, which flux does not track yet: `&` without a
/// lifetime is an error there (E0637), as in rustc, and a lifetime
/// argument is one too, since traits have no lifetime parameters. In a
/// `Fn`-family bound, elided lifetimes among the parameters are its own
/// (higher-ranked), and supported.
fn check_bounds<'b>(
    generics: &'b Generics,
    more: impl Iterator<Item = &'b flux_hir::Bound>,
    errors: &mut Vec<(TextRange, String)>,
) {
    for bound in generics.predicates.iter().map(|p| &p.bound).chain(more) {
        for args in bound.path.args.iter().flatten() {
            if let Some(&first) = args.lifetimes.first() {
                errors.push((first.range, no_lifetimes("trait", args.lifetimes.len())));
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
                for b in args.bindings.iter() {
                    let mut found = Vec::new();
                    elided_in(&b.ty, &mut found);
                    lifetimes_in(&b.ty, &mut named);
                    if !found.is_empty()
                        || named.iter().any(|l| {
                            b.ty_range.start() <= l.range.start()
                                && l.range.end() <= b.ty_range.end()
                        })
                    {
                        errors.push((
                            b.ty_range,
                            "`Fn` bounds returning references are not supported yet".to_owned(),
                        ));
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
                lifetimes_in(&b.ty, &mut named);
            }
            for at in elided {
                errors.push((
                    at,
                    "`&` without an explicit lifetime name cannot be used here".to_owned(),
                ));
            }
            if let Some(l) = named.iter().find(|l| !l.is_anonymous()) {
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
                implied_outlives(items, sigs, ty, regions, &mut out);
            }
            out.retain(|p| match *p {
                Outlives::Region(a, b) => {
                    a != Region::Erased && b != Region::Erased && a != b && a != Region::Static
                }
                Outlives::Type(_, b) => b != Region::Erased && b != Region::Static,
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
