//! Region-aware well-formedness of bounded nominal types.
use super::*;

#[derive(Clone)]
struct AdtUse {
    strukt: flux_hir::StructId,
    args: Vec<Ty>,
    lifetimes: Vec<Var>,
    types: Vec<Vec<Var>>,
}

fn has_contract(items: &ItemTree, sigs: &Signatures, strukt: flux_hir::StructId) -> bool {
    let Some(metadata) = sigs.regions.structs.get(strukt) else {
        return false;
    };
    metadata.bound_selfs.iter().any(|row| !row.is_empty())
        || metadata
            .bound_traits
            .iter()
            .any(|row| !row.lifetimes.is_empty() || row.args.iter().any(|row| !row.is_empty()))
        || metadata.projections.iter().any(|row| !row.is_empty())
        || sigs
            .struct_predicates
            .get(strukt)
            .is_some_and(|predicates| {
                predicates.iter().any(|p| {
                    items.traits[p.trait_ref.trait_].lang != Some(flux_intern::sym::sized)
                        && (p.self_ty.has_params()
                            || p.trait_ref.args.iter().any(|ty| ty.has_params()))
                })
            })
}

/// Only argument rows used by a predicate can affect its lifetime proof.
/// A borrowed element T does not make an unrelated A: Allocator obligation
/// region-bearing when A is the concrete global allocator.
fn instance_has_contract(
    items: &ItemTree,
    sigs: &Signatures,
    strukt: flux_hir::StructId,
    args: &[Ty],
) -> bool {
    if !has_contract(items, sigs, strukt) {
        return false;
    }
    let metadata = &sigs.regions.structs[strukt];
    if metadata
        .bound_traits
        .iter()
        .any(|row| !row.lifetimes.is_empty())
    {
        return true;
    }
    let slots = |ty: Ty| flux_ty::regions::slots(items, ty.subst(args));
    sigs.struct_predicates
        .get(strukt)
        .is_some_and(|predicates| {
            predicates.iter().any(|p| {
                let self_ty = p.self_ty.subst(args);
                items.traits[p.trait_ref.trait_].lang != Some(flux_intern::sym::sized)
                    && !flux_solve::traits::is_builtin_trait(items, p.trait_ref.trait_, self_ty)
                    && (flux_ty::regions::slots(items, self_ty) != 0
                        || p.trait_ref.args.iter().any(|&ty| slots(ty) != 0))
            })
        })
        || sigs
            .struct_projections
            .get(strukt)
            .is_some_and(|projections| {
                projections
                    .iter()
                    .any(|p| slots(Ty::alias(p.alias)) != 0 || slots(p.ty) != 0)
            })
}

fn type_has_adt_contract(items: &ItemTree, sigs: &Signatures, ty: Ty) -> bool {
    // Types form a finite interned graph. A nesting limit must not turn a
    // deep scalar tuple into a nominal lifetime obligation.
    let mut pending = vec![ty];
    let mut visited = rustc_hash::FxHashSet::default();
    while let Some(ty) = pending.pop() {
        if !visited.insert(ty) {
            continue;
        }
        if matches!(ty.kind(), TyKind::Adt(adt, args) if instance_has_contract(items, sigs, adt.strukt, args))
        {
            return true;
        }
        pending.extend(ty.children());
    }
    false
}

impl Cx<'_> {
    /// Walk type arguments, never nominal field definitions. Fields are checked
    /// once at their declaration, so recursive nominal graphs stay bounded.
    fn adt_uses(&mut self, ty: Ty, vars: &[Var], out: &mut Vec<AdtUse>, depth: u32) -> bool {
        if depth > 64 {
            return false;
        }
        if vars.len() != self.slots(ty) as usize {
            return false;
        }
        match ty.kind() {
            TyKind::Adt(adt, args) => {
                let n = self.items.structs[adt.strukt].generics.lifetimes.len();
                let Some(lifetimes) = vars.get(..n) else {
                    return false;
                };
                let mut next = n;
                let mut types = Vec::new();
                for &arg in args {
                    let count = self.slots(arg) as usize;
                    let Some(row) = vars.get(next..next + count) else {
                        return false;
                    };
                    if !self.adt_uses(arg, row, out, depth + 1) {
                        return false;
                    }
                    types.push(row.to_vec());
                    next += count;
                }
                if self.sigs.regions.structs.get(adt.strukt).is_none() {
                    return false;
                }
                if instance_has_contract(self.items, self.sigs, adt.strukt, args) {
                    out.push(AdtUse {
                        strukt: adt.strukt,
                        args: args.to_vec(),
                        lifetimes: lifetimes.to_vec(),
                        types,
                    });
                }
            }
            TyKind::Ref(_, inner) => return self.adt_uses(inner, &vars[1..], out, depth + 1),
            TyKind::RawPtr(_, inner) | TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                return self.adt_uses(inner, vars, out, depth + 1);
            }
            TyKind::Tuple(elements) => {
                let mut next = 0;
                for &elem in elements {
                    let n = self.slots(elem) as usize;
                    let Some(row) = vars.get(next..next + n) else {
                        return false;
                    };
                    if !self.adt_uses(elem, row, out, depth + 1) {
                        return false;
                    }
                    next += n;
                }
            }
            TyKind::Closure(_, args) => {
                return self.adt_uses(
                    flux_ty::ClosureArgs::split(args).upvars,
                    vars,
                    out,
                    depth + 1,
                );
            }
            TyKind::Opaque(opaque) => {
                let mut next = opaque.lifetimes as usize;
                for &arg in opaque.args {
                    let n = self.slots(arg) as usize;
                    let Some(row) = vars.get(next..next + n) else {
                        return false;
                    };
                    if !self.adt_uses(arg, row, out, depth + 1) {
                        return false;
                    }
                    next += n;
                }
            }
            TyKind::Alias(alias) if flux_ty::regions::has_projection_inputs(self.items, alias) => {
                let mut next = alias.owner_lifetimes as usize;
                for &arg in alias.args {
                    let n = self.slots(arg) as usize;
                    let Some(row) = vars.get(next..next + n) else {
                        return false;
                    };
                    if !self.adt_uses(arg, row, out, depth + 1) {
                        return false;
                    }
                    next += n;
                }
                next += alias.family_lifetimes as usize;
                for &arg in alias.family_args {
                    let n = self.slots(arg) as usize;
                    let Some(row) = vars.get(next..next + n) else {
                        return false;
                    };
                    if !self.adt_uses(arg, row, out, depth + 1) {
                        return false;
                    }
                    next += n;
                }
            }
            // The value row represents free captures, not universally bound
            // lifetimes in the pointer's argument types.
            TyKind::FnPtr(signature)
                if signature
                    .inputs_output
                    .iter()
                    .any(|&ty| self.type_has_adt_contract(ty)) =>
            {
                return false;
            }
            _ => {}
        }
        true
    }

    fn type_has_adt_contract(&self, ty: Ty) -> bool {
        type_has_adt_contract(self.items, self.sigs, ty)
    }

    pub(super) fn adt_obligations(&mut self, ty: Ty, vars: &[Var], range: TextRange) {
        if !self.type_has_adt_contract(ty) {
            return;
        }
        let mut uses = Vec::new();
        if !self.adt_uses(ty, vars, &mut uses, 0) {
            self.errors.push(Diagnostic::new(
                range,
                "ADT trait lifetime requirements are not represented at this use",
            ));
            return;
        }
        for use_ in uses {
            let metadata = self.sigs.regions.structs[use_.strukt].clone();
            let predicates = self
                .sigs
                .struct_predicates
                .get(use_.strukt)
                .map_or(&[][..], |p| p)
                .to_vec();
            let projections = self
                .sigs
                .struct_projections
                .get(use_.strukt)
                .map_or(&[][..], |p| p)
                .to_vec();
            if metadata.bound_selfs.len() != predicates.len()
                || metadata.bound_traits.len() != predicates.len()
                || metadata.projections.len() != projections.len()
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "ADT trait lifetime metadata is incomplete",
                ));
                continue;
            }
            if !self.callable_outputs_complete(
                &self.items.structs[use_.strukt].generics,
                &predicates,
                &projections,
                range,
            ) {
                continue;
            }
            let previous = std::mem::replace(
                &mut self.call_projections,
                projections
                    .iter()
                    .copied()
                    .zip(metadata.projections.iter().cloned())
                    .collect(),
            );
            for (index, &predicate) in predicates.iter().enumerate() {
                self.prove_bound(
                    predicate,
                    &metadata.bound_selfs[index],
                    &metadata.bound_traits[index],
                    &use_.lifetimes,
                    &use_.args,
                    &use_.types,
                    range,
                    0,
                );
            }
            self.call_projections = previous;
            let bounds: Vec<_> = predicates
                .iter()
                .copied()
                .zip(metadata.bound_selfs.iter())
                .zip(metadata.bound_traits.iter())
                .map(|((predicate, own), rows)| (predicate, &**own, rows.clone()))
                .collect();
            for (index, (&projection, table)) in projections
                .iter()
                .zip(metadata.projections.iter())
                .enumerate()
            {
                let input = self.declared_projection_input(
                    &self.items.structs[use_.strukt].generics,
                    &projections,
                    index,
                    &bounds,
                    range,
                );
                self.prove_projection(
                    projection,
                    table,
                    input.as_ref(),
                    &use_.lifetimes,
                    &use_.args,
                    &use_.types,
                    range,
                );
            }
        }
    }

    /// A declaration check has no runtime caller. Its original predicates are
    /// universal assumptions, while every nested ADT contract is an obligation.
    pub(super) fn adt_declaration_uses(
        &mut self,
        ty: Ty,
        vars: &[Var],
        env: &[ImplRegionGoal],
    ) -> Option<bool> {
        // The supplied row describes the instantiated output, not captures
        // of projections that normalization has already replaced.
        let ty = self.normalize(ty);
        let mut uses = Vec::new();
        if !self.adt_uses(ty, vars, &mut uses, 0) {
            return None;
        }
        for use_ in uses {
            let metadata = self.sigs.regions.structs.get(use_.strukt)?;
            let predicates = self
                .sigs
                .struct_predicates
                .get(use_.strukt)
                .map_or(&[][..], |p| p);
            if metadata.bound_selfs.len() != predicates.len()
                || metadata.bound_traits.len() != predicates.len()
            {
                return None;
            }
            for (index, &predicate) in predicates.iter().enumerate() {
                let goal = self.impl_region_goal_with_inputs(
                    predicate,
                    &metadata.bound_selfs[index],
                    &metadata.bound_traits[index],
                    &use_.lifetimes,
                    &use_.args,
                    &use_.types,
                    None,
                    false,
                    Some(env),
                )?;
                if !self.impl_prove_super(&goal, env, 0) {
                    return Some(false);
                }
            }
            let projections = self
                .sigs
                .struct_projections
                .get(use_.strukt)
                .map_or(&[][..], |p| p);
            if metadata.projections.len() != projections.len() {
                return None;
            }
            if !self.callable_outputs_complete(
                &self.items.structs[use_.strukt].generics,
                predicates,
                projections,
                TextRange::default(),
            ) {
                return Some(false);
            }
            // Trait-lifetime projection equalities need their own selection
            // input rows. Retain an explicit boundary rather than erasing them.
            let bounds: Vec<_> = predicates
                .iter()
                .copied()
                .zip(metadata.bound_selfs.iter())
                .zip(metadata.bound_traits.iter())
                .map(|((predicate, own), rows)| (predicate, &**own, rows.clone()))
                .collect();
            for (index, (projection, table)) in projections
                .iter()
                .zip(metadata.projections.iter())
                .enumerate()
            {
                if flux_ty::regions::has_projection_inputs(self.items, projection.alias) {
                    return None;
                }
                let input = self.declared_projection_input(
                    &self.items.structs[use_.strukt].generics,
                    projections,
                    index,
                    &bounds,
                    TextRange::default(),
                );
                self.prove_projection(
                    *projection,
                    table,
                    input.as_ref(),
                    &use_.lifetimes,
                    &use_.args,
                    &use_.types,
                    TextRange::default(),
                );
            }
        }
        Some(true)
    }
}

fn predicate_has_adt_contract(items: &ItemTree, sigs: &Signatures, p: flux_ty::Predicate) -> bool {
    std::iter::once(p.self_ty)
        .chain(p.trait_ref.args.iter().copied())
        .any(|ty| type_has_adt_contract(items, sigs, ty))
}

impl Cx<'_> {
    fn adt_environment_uses(&mut self, env: &[ImplRegionGoal]) -> bool {
        for goal in env {
            for (ty, row) in std::iter::once(goal.predicate.self_ty)
                .chain(goal.predicate.trait_ref.args.iter().copied())
                .zip(&goal.vars)
            {
                if self.adt_declaration_uses(ty, row, env) != Some(true) {
                    return false;
                }
            }
        }
        true
    }
}

pub(crate) fn check_adt_declarations(
    items: &ItemTree,
    sigs: &Signatures,
    from: flux_hir::ItemCounts,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    let body = Body::empty(None);
    let none = Requirements::default();
    for (id, data) in items.structs.iter().skip(from.structs as usize) {
        if !sigs.structs[id]
            .iter()
            .any(|&ty| type_has_adt_contract(items, sigs, ty))
            && !sigs.struct_predicates.get(id).is_some_and(|p| {
                p.iter()
                    .any(|&p| predicate_has_adt_contract(items, sigs, p))
            })
            && !sigs
                .struct_projections
                .get(id)
                .is_some_and(|p| p.iter().any(|p| type_has_adt_contract(items, sigs, p.ty)))
        {
            continue;
        }
        let Some(metadata) = sigs.regions.structs.get(id) else {
            continue;
        };
        let mut cx = Cx::new(&body, items, sigs, &none);
        cx.new_universal("'static".into());
        let lifetimes: Vec<_> = (0..metadata.lifetimes)
            .map(|i| cx.new_universal(format!("'{i}")))
            .collect();
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        cx.env = sigs
            .struct_projections
            .get(id)
            .map_or(Vec::new(), |p| p.to_vec());
        cx.header_predicates = sigs
            .struct_predicates
            .get(id)
            .map_or(Vec::new(), |p| flux_solve::traits::elaborate(sigs, p));
        for bound in &metadata.outlives {
            match *bound {
                Outlives::Region(a, b) => {
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        cx.known_pairs.push((a, b))
                    }
                }
                Outlives::Type(ty, b) => {
                    if let Some(b) = map(b) {
                        cx.known_types.push((ty, b))
                    }
                }
            }
        }
        let mut env = Vec::new();
        let mut complete = true;
        for (index, &predicate) in sigs
            .struct_predicates
            .get(id)
            .map_or(&[][..], |p| p)
            .iter()
            .enumerate()
        {
            let goal = metadata
                .bound_selfs
                .get(index)
                .zip(metadata.bound_traits.get(index))
                .and_then(|(s, t)| cx.impl_region_goal(predicate, s, t, &lifetimes, &[], &[]));
            match goal {
                Some(goal) => {
                    complete &= cx.impl_super_goals(goal, &mut env, 0);
                }
                None => complete = false,
            }
        }
        for (projection, table) in cx.env.clone().iter().zip(metadata.projections.iter()) {
            if let Some(row) = cx.instantiate_sig(projection.ty, table, &mut { map }, &[], &[]) {
                cx.env_regions.push((projection.alias, row));
            } else {
                complete = false;
            }
        }
        cx.family_goals = env.clone();
        cx.close_known();
        complete &= cx.adt_environment_uses(&env);
        for (projection, row) in cx.env.clone().iter().zip(cx.env_regions.clone().iter()) {
            complete &= cx.adt_declaration_uses(projection.ty, &row.1, &env) == Some(true);
        }
        for (&ty, table) in sigs.structs[id].iter().zip(metadata.fields.iter()) {
            let row = cx.instantiate_sig(ty, table, &mut { map }, &[], &[]);
            match row.and_then(|row| cx.adt_declaration_uses(cx.normalize(ty), &row, &env)) {
                Some(true) => {}
                Some(false) => {
                    complete = false;
                    break;
                }
                None => {
                    complete = false;
                    break;
                }
            }
        }
        let solution = cx.solve(&Points::new(&body), &[], &[]);
        if !complete
            || !cx.check_universal(&solution).is_empty()
            || !cx.check_type_tests(&solution).is_empty()
            || !cx.errors.is_empty()
        {
            diagnostics.push(Diagnostic {
                file: data.loc.file,
                range: data.def_range,
                message:
                    "nested ADT trait lifetime requirements are not satisfied by the declaration"
                        .into(),
            });
        }
    }
    for (imp, data) in items.impls.iter().skip(from.impls as usize) {
        let values = sigs.impl_types.get(imp).map_or(&[][..], |values| values);
        if !type_has_adt_contract(items, sigs, sigs.impls[imp])
            && !values
                .iter()
                .any(|&(_, ty)| type_has_adt_contract(items, sigs, ty))
            && !sigs.impl_predicates.get(imp).is_some_and(|p| {
                p.iter()
                    .any(|&p| predicate_has_adt_contract(items, sigs, p))
            })
            && !sigs
                .impl_projections
                .get(imp)
                .is_some_and(|p| p.iter().any(|p| type_has_adt_contract(items, sigs, p.ty)))
        {
            continue;
        }
        let Some(metadata) = sigs.regions.impls.get(imp) else {
            continue;
        };
        let mut cx = Cx::new(&body, items, sigs, &none);
        let Some((lifetimes, env)) = cx.impl_header_environment(imp) else {
            diagnostics.push(Diagnostic {
                file: data.file,
                range: data.self_range,
                message: "ADT trait lifetime requirements are not represented in this impl header"
                    .into(),
            });
            continue;
        };
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        cx.close_known();
        let ty = sigs.impls[imp];
        let mut result = cx
            .instantiate_sig(ty, &metadata.self_ty, &mut { map }, &[], &[])
            .and_then(|row| cx.adt_declaration_uses(cx.normalize(ty), &row, &env));
        if !cx.adt_environment_uses(&env) {
            result = Some(false);
        }
        for (projection, row) in cx.env.clone().iter().zip(cx.env_regions.clone().iter()) {
            if cx.adt_declaration_uses(projection.ty, &row.1, &env) != Some(true) {
                result = Some(false);
            }
        }
        for &(name, ty) in values {
            if !type_has_adt_contract(items, sigs, ty) {
                continue;
            }
            // The existing family proof checks this under its separate fresh
            // local binders and original family predicates.
            if data.types.iter().any(|t| {
                t.name == name
                    && (!t.generics.lifetimes.is_empty() || !t.generics.params.is_empty())
            }) {
                continue;
            }
            let row = metadata
                .types
                .iter()
                .find(|(n, _)| *n == name)
                .and_then(|(_, table)| cx.instantiate_sig(ty, table, &mut { map }, &[], &[]));
            if row.and_then(|row| cx.adt_declaration_uses(ty, &row, &env)) != Some(true) {
                result = Some(false);
            }
        }
        let solution = cx.solve(&Points::new(&body), &[], &[]);
        if result != Some(true)
            || !cx.check_universal(&solution).is_empty()
            || !cx.check_type_tests(&solution).is_empty()
            || !cx.errors.is_empty()
        {
            diagnostics.push(Diagnostic {
                file: data.file,
                range: data.self_range,
                message: "ADT trait lifetime requirements are not satisfied by the impl header"
                    .into(),
            });
        }
    }
    // Required methods and foreign declarations have no MIR body. Their
    // nominal signature requirements still hold universally at declaration.
    for (function, data) in items.functions.iter().skip(from.functions as usize) {
        let Some(signature) = sigs.fns.get(function) else {
            continue;
        };
        let bound_use = sigs.fn_predicates.get(function).is_some_and(|p| {
            p.iter()
                .any(|&p| predicate_has_adt_contract(items, sigs, p))
        }) || sigs
            .fn_projections
            .get(function)
            .is_some_and(|p| p.iter().any(|p| type_has_adt_contract(items, sigs, p.ty)));
        if !bound_use
            && (data.has_body
                || !signature
                    .params
                    .iter()
                    .chain(std::iter::once(&signature.ret))
                    .any(|&ty| type_has_adt_contract(items, sigs, ty)))
        {
            continue;
        }
        let Some(metadata) = sigs.regions.fns.get(function) else {
            continue;
        };
        let mut cx = Cx::new(&body, items, sigs, &none);
        let owner_impl = match data.owner {
            Some(flux_hir::Owner::Impl(imp)) => Some(imp),
            _ => None,
        };
        let (mut lifetimes, mut env, mut complete) = if let Some(imp) = owner_impl {
            match cx.impl_header_environment(imp) {
                Some((lifetimes, env)) => {
                    let complete = lifetimes.len() == metadata.owner as usize;
                    (lifetimes, env, complete)
                }
                None => {
                    if cx.universal.is_empty() {
                        cx.new_universal("'static".into());
                    }
                    (Vec::new(), Vec::new(), false)
                }
            }
        } else {
            cx.new_universal("'static".into());
            (Vec::new(), Vec::new(), true)
        };
        for i in lifetimes.len()..metadata.names.len() {
            lifetimes.push(cx.new_universal(format!("'{i}")));
        }
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            _ => None,
        };
        let own_projections = sigs.fn_projections.get(function).map_or(&[][..], |p| p);
        cx.env.extend(own_projections.iter().copied());
        cx.header_predicates.extend(
            sigs.fn_predicates
                .get(function)
                .map_or(Vec::new(), |p| flux_solve::traits::elaborate(sigs, p)),
        );
        for &bound in &metadata.outlives {
            match bound {
                Outlives::Region(a, b) => {
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        cx.known_pairs.push((a, b));
                    }
                }
                Outlives::Type(ty, b) => {
                    if let Some(b) = map(b) {
                        cx.known_types.push((ty, b));
                    }
                }
            }
        }
        for (index, &predicate) in sigs
            .fn_predicates
            .get(function)
            .map_or(&[][..], |p| p)
            .iter()
            .enumerate()
        {
            let original_self = owner_impl.and_then(|imp| {
                let written = data.generics.predicates.get(index)?;
                if !matches!(&written.ty, flux_hir::TypeRef::Path(path)
                    if path.as_name()==Some(flux_intern::kw::SelfUpper))
                {
                    return None;
                }
                if predicate.self_ty != sigs.impls[imp] {
                    return None;
                }
                sigs.regions.impls.get(imp).map(|owner| &owner.self_ty)
            });
            let self_row = original_self.or_else(|| metadata.bound_selfs.get(index));
            let goal = self_row
                .zip(metadata.bound_traits.get(index))
                .and_then(|(s, t)| cx.impl_region_goal(predicate, s, t, &lifetimes, &[], &[]));
            match goal {
                Some(goal) => complete &= cx.impl_super_goals(goal, &mut env, 0),
                None => complete = false,
            }
        }
        for (projection, table) in own_projections.iter().zip(metadata.projections.iter()) {
            if let Some(row) = cx.instantiate_sig(projection.ty, table, &mut { map }, &[], &[]) {
                cx.env_regions.push((projection.alias, row));
            } else {
                complete = false;
            }
        }
        cx.family_goals = env.clone();
        cx.close_known();
        complete &= cx.adt_environment_uses(&env);
        for (projection, row) in cx.env.clone().iter().zip(cx.env_regions.clone().iter()) {
            complete &= cx.adt_declaration_uses(projection.ty, &row.1, &env) == Some(true);
        }
        for (&ty, table) in signature
            .params
            .iter()
            .chain(std::iter::once(&signature.ret))
            .zip(metadata.params.iter().chain(std::iter::once(&metadata.ret)))
        {
            let row = cx.instantiate_sig(ty, table, &mut { map }, &[], &[]);
            if row.and_then(|row| cx.adt_declaration_uses(ty, &row, &env)) != Some(true) {
                complete = false;
            }
        }
        let solution = cx.solve(&Points::new(&body), &[], &[]);
        if !complete
            || !cx.check_universal(&solution).is_empty()
            || !cx.check_type_tests(&solution).is_empty()
            || !cx.errors.is_empty()
        {
            diagnostics.push(Diagnostic {
                file: data.loc.file,
                range: data.item_range,
                message:
                    "ADT trait lifetime requirements are not satisfied by the function declaration"
                        .into(),
            });
        }
    }
    // Initializer bodies have no function owner. Check the written constant
    // signature under its original impl owner, even for an unused `None`.
    for (konst, data) in items.consts.iter().skip(from.consts as usize) {
        let Some(&ty) = sigs.consts.get(konst) else {
            continue;
        };
        if !type_has_adt_contract(items, sigs, ty) {
            continue;
        }
        let Some(table) = sigs.regions.consts.get(konst) else {
            continue;
        };
        let mut cx = Cx::new(&body, items, sigs, &none);
        let owner = match data.owner {
            Some(flux_hir::Owner::Impl(imp)) => cx.impl_header_environment(imp),
            None => {
                cx.new_universal("'static".into());
                Some((Vec::new(), Vec::new()))
            }
            Some(flux_hir::Owner::Trait(_)) => None,
        };
        let valid = if let Some((lifetimes, env)) = owner {
            let map = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied(),
                _ => None,
            };
            cx.close_known();
            let result = cx
                .instantiate_sig(ty, table, &mut { map }, &[], &[])
                .and_then(|row| cx.adt_declaration_uses(ty, &row, &env));
            let solution = cx.solve(&Points::new(&body), &[], &[]);
            result == Some(true)
                && cx.check_universal(&solution).is_empty()
                && cx.check_type_tests(&solution).is_empty()
                && cx.errors.is_empty()
        } else {
            false
        };
        if !valid {
            diagnostics.push(Diagnostic {
                file: data.loc.file,
                range: data.ty_range,
                message:
                    "ADT trait lifetime requirements are not satisfied by the constant declaration"
                        .into(),
            });
        }
    }
    diagnostics
}
