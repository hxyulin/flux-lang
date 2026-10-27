//! Resolve and validate item signatures, extending the frozen library tables.
use super::{
    ArgTys, ArrayLens, Builtins, Core, Diagnostic, FnSig, FxHashMap, Generics, ItemCounts, ItemLoc,
    ItemTree, LenValues, Mutability, OnceLock, Owner, Predicate, Signatures, Symbol, TextRange,
    TraitSig, Ty, TyKind, TypeRef, TypeScope, check_alias, check_const_params, check_type_defaults,
    contains, contains_ref, core_lens_values, discriminant_ty, duplicate_assoc_items,
    generic_consts, generic_names, impl_index, impl_type, in_scope, is_lang_struct, kw,
    maybe_unsized, mentions_param, not_sync, opaque_sigs, param_const_ty, params_of, regions,
    resolve_generic_arg, resolve_or_report, resolve_predicates, resolve_type, self_ty,
    supported_union_field, traits, unsized_argument, unsized_error, unsized_params, uses_param,
};

/// The signature of every function and the field types of every struct
/// in `items`.
pub fn signatures(items: &ItemTree) -> (Signatures, Vec<Diagnostic>) {
    signatures_checked(items, false, None)
}

/// Like [`signatures`]; `check_core` checks `core`'s `impl` blocks
/// against their traits too, which its tests do instead of every build.
///
/// `core`'s signatures are resolved once per process; a program's are
/// resolved after them, unless `check_core` resolves everything again.
///
/// The lengths of array types named by constants (`[u8; N]`) are taken
/// from `lens`; those it does not know make the types errors, and are
/// noted in it as missing.
pub fn signatures_checked(
    items: &ItemTree,
    check_core: bool,
    lens: Option<&ArrayLens<'_>>,
) -> (Signatures, Vec<Diagnostic>) {
    let core = items.core;
    if check_core || core == ItemCounts::default() {
        let checked = if check_core { 0 } else { core.impls };
        // `core`'s lengths are known without evaluation, as where its
        // signatures are shared.
        let known = match lens {
            None if core != ItemCounts::default() => core_lens_values(),
            _ => LenValues::default(),
        };
        let arg_tys = ArgTys::default();
        let core_lens = ArrayLens::new(&known, &arg_tys);
        let (mut sigs, diagnostics) = extend_signatures(
            items,
            Signatures::default(),
            ItemCounts::default(),
            checked,
            lens.or(Some(&core_lens)),
        );
        if lens.is_none() {
            sigs.array_lens = known;
        }
        return (sigs, diagnostics);
    }
    let (core_sigs, core_diagnostics) = core_signatures();
    let sigs = core_sigs.fork_for_extension();
    let mut diagnostics = core_diagnostics.clone();
    let (sigs, more) = extend_signatures(items, sigs, core, core.impls, lens);
    diagnostics.extend(more);
    (sigs, diagnostics)
}

/// The signatures of [`Core`]'s items, its `impl` blocks unchecked as in
/// every build, resolved the first time they are asked for.
pub(super) fn core_signatures() -> &'static (Signatures, Vec<Diagnostic>) {
    static CORE: OnceLock<(Signatures, Vec<Diagnostic>)> = OnceLock::new();
    CORE.get_or_init(|| {
        let items = &Core::get().items;
        let unchecked = items.impls.len() as u32;
        let known = core_lens_values();
        let arg_tys = ArgTys::default();
        let lens = ArrayLens::new(&known, &arg_tys);
        let (mut sigs, diagnostics) = extend_signatures(
            items,
            Signatures::default(),
            ItemCounts::default(),
            unchecked,
            Some(&lens),
        );
        debug_assert!(
            lens.missing.borrow().is_empty(),
            "core's array lengths name only constants whose values are literals"
        );
        sigs.array_lens = known;
        sigs.freeze();
        (sigs, diagnostics)
    })
}

/// Resolves the signatures of `items` from `from` on, given `sigs`, the
/// signatures of those before, which do not depend on them, and checks
/// them; the `impl` blocks from `checked_impls` on are checked against
/// their traits.
pub(super) fn extend_signatures(
    items: &ItemTree,
    mut sigs: Signatures,
    from: ItemCounts,
    checked_impls: u32,
    lens: Option<&ArrayLens<'_>>,
) -> (Signatures, Vec<Diagnostic>) {
    sigs.traits_in_scope = in_scope::TraitsInScope::new(items);
    sigs.builtins = Builtins::of(items);
    let mut diagnostics = Vec::new();
    let mut error = |file, range, message: &str| {
        let mut d = Diagnostic::new(range, message);
        d.file = file;
        diagnostics.push(d);
    };
    let mut reported = Vec::new();
    fn resolve(
        items: &ItemTree,
        loc: ItemLoc,
        ty: &TypeRef,
        scope: TypeScope<'_>,
        range: TextRange,
        reported: &mut Vec<Diagnostic>,
    ) -> Ty {
        let start = reported.len();
        let ty = resolve_or_report(items, loc.module, ty, scope, range, reported);
        for d in &mut reported[start..] {
            d.file = loc.file;
        }
        ty
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let ty = impl_type(items, imp, lens, &mut reported);
        sigs.impls.insert(id, ty);
    }
    sigs.impl_index = impl_index::ImplIndex::new(items, &sigs);
    let in_file = |errors: Vec<(TextRange, String)>, file, reported: &mut Vec<Diagnostic>| {
        reported.extend(errors.into_iter().map(|(range, message)| Diagnostic {
            file,
            range,
            message,
        }));
    };
    // Traits' defaults first: bounds anywhere may leave them out.
    let mut defaults = std::mem::take(&mut sigs.trait_defaults);
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let generics = generic_names(items, Some(Owner::Trait(id)), &Generics::default());
        let consts = generic_consts(items, Some(Owner::Trait(id)), &Generics::default());
        let scope = TypeScope {
            self_ty: Some(Ty::param(0, kw::SelfUpper)),
            generics: &generics,
            consts: &consts,
            signature_of: Some("traits"),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let resolved: Box<[Option<Ty>]> = t
            .generics
            .params
            .iter()
            .map(|p| {
                let (ty, range) = p.default.as_ref()?;
                Some(resolve_generic_arg(
                    items,
                    t.loc.module,
                    Some(p),
                    ty,
                    scope,
                    *range,
                    &mut errors,
                ))
            })
            .collect();
        in_file(errors, t.loc.file, &mut reported);
        defaults.insert(id, resolved);
    }
    sigs.trait_defaults = defaults;
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        let generics = generic_names(items, Some(Owner::Trait(id)), &Generics::default());
        let consts = generic_consts(items, Some(Owner::Trait(id)), &Generics::default());
        let self_param = Ty::param(0, kw::SelfUpper);
        let bounds = [traits::trait_self_predicate(items, id)];
        let mut errors = Vec::new();
        check_const_params(items, t.loc.module, &t.generics, &generics, &mut errors);
        in_file(errors, t.loc.file, &mut reported);
        let scope = TypeScope {
            self_ty: Some(self_param),
            generics: &generics,
            consts: &consts,
            signature_of: Some("traits"),
            bounds: &bounds,
            defaults: Some(&sigs.trait_defaults),
            struct_fields: Some(&sigs.structs),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let mut supertraits = Vec::new();
        let mut projections = Vec::new();
        let mut implied = Vec::new();
        for b in t.supertraits.iter() {
            if let Some((trait_ref, bindings)) =
                traits::resolve_bound(items, t.loc.module, b, self_param, scope, &mut errors)
            {
                supertraits.push(trait_ref);
                implied.extend(bindings.iter().copied());
                projections.extend(bindings);
            }
        }
        let mut predicates = Vec::new();
        let resolved =
            resolve_predicates(items, &sigs, t.loc.module, &t.generics, scope, &mut errors);
        // `where Self: Sup` is a supertrait too.
        for p in resolved.predicates {
            if p.self_ty == self_param {
                supertraits.push(p.trait_ref);
            } else {
                predicates.push(p);
            }
        }
        implied.extend(
            resolved
                .projections
                .iter()
                .filter(|p| p.alias.args[0] == self_param),
        );
        projections.extend(resolved.projections);
        let header_projections = projections.clone().into_boxed_slice();
        let mut type_bounds = Vec::new();
        for ty in &t.types {
            let alias = Ty::alias(traits::alias_of(bounds[0].trait_ref, self_param, ty.name));
            for b in ty.bounds.iter() {
                if let Some((trait_ref, bindings)) =
                    traits::resolve_bound(items, t.loc.module, b, alias, scope, &mut errors)
                {
                    type_bounds.push((ty.name, trait_ref));
                    implied.extend(bindings.iter().copied());
                    projections.extend(bindings);
                }
            }
        }
        in_file(errors, t.loc.file, &mut reported);
        let defaults = sigs.trait_defaults.get(id).cloned().unwrap_or_default();
        sigs.traits.insert(
            id,
            TraitSig {
                supertraits: supertraits.into(),
                defaults,
                type_bounds: type_bounds.into(),
                implied: implied.into(),
                header_projections,
            },
        );
        sigs.trait_predicates.insert(id, predicates.into());
        if !projections.is_empty() {
            sigs.trait_projections.insert(id, projections.into());
        }
    }
    // Structs' bounds, then their fields, in which `T::Out` may name
    // the associated type of a bound on `T`.
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        if s.is_union {
            sigs.unions.insert(id);
        }
        let generics: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let consts: Vec<Option<Ty>> = s.generics.params.iter().map(param_const_ty).collect();
        let unsized_params = unsized_params(items, None, &s.generics);
        let scope = TypeScope {
            self_ty: Some(Ty::adt(id, s.name, &params_of(&s.generics, 0))),
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("structs"),
            defaults: Some(&sigs.trait_defaults),
            struct_fields: Some(&sigs.structs),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        check_const_params(items, s.loc.module, &s.generics, &generics, &mut errors);
        let own = resolve_predicates(items, &sigs, s.loc.module, &s.generics, scope, &mut errors);
        in_file(errors, s.loc.file, &mut reported);
        let bounds = traits::elaborate(&sigs, &own.predicates);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
        let mut default_errors = Vec::new();
        check_type_defaults(items, &s.generics, s.loc.module, scope, &mut default_errors);
        in_file(default_errors, s.loc.file, &mut reported);
        let mut starts = Vec::new();
        let mut fields: Vec<Ty> = Vec::new();
        for v in 0..s.variant_count() as u32 {
            starts.push(fields.len() as u32);
            let variant = s.variant(v).fields;
            fields.extend(
                variant
                    .iter()
                    .map(|f| resolve(items, s.loc, &f.ty, scope, f.range, &mut reported)),
            );
        }
        starts.push(fields.len() as u32);
        sigs.structs.insert(id, Ty::list(&fields));
        if s.is_enum() {
            sigs.variant_starts.insert(id, starts.into());
            sigs.discriminants.insert(id, discriminant_ty(items, id));
        }
        if !own.predicates.is_empty() {
            sigs.struct_predicates.insert(id, own.predicates.into());
        }
        if !own.projections.is_empty() {
            sigs.struct_projections.insert(id, own.projections.into());
        }
    }
    for (id, data) in items.structs.iter().skip(from.structs as usize) {
        let parameters = unsized_params(items, None, &data.generics);
        let bounds = traits::elaborate(
            &sigs,
            sigs.struct_predicates
                .get(id)
                .map_or(&[][..], |bounds| bounds),
        );
        for variant in 0..data.variant_count() as u32 {
            let fields = sigs
                .variant_fields(
                    Ty::adt(id, data.name, &params_of(&data.generics, 0)),
                    variant,
                )
                .unwrap_or_default();
            for (index, &ty) in fields.iter().enumerate() {
                let scope = TypeScope {
                    struct_fields: Some(&sigs.structs),
                    unsized_params: &parameters,
                    bounds: &bounds,
                    ..TypeScope::default()
                };
                if let Some(arg) = unsized_argument(items, scope, ty) {
                    let mut error = Diagnostic::new(
                        data.variant(variant).fields[index].range,
                        unsized_error(arg),
                    );
                    error.file = data.loc.file;
                    reported.push(error);
                }
                if maybe_unsized(items, &sigs, ty, &parameters, &bounds) {
                    let valid_tail = !data.is_enum() && !data.is_union && index + 1 == fields.len();
                    let tail = sigs.structural_tail(ty);
                    let message = if !valid_tail {
                        Some("only the last field of a struct may be unsized")
                    } else if matches!(tail.kind(), TyKind::Dynamic(_)) {
                        Some("trait object tails in structs are not supported yet")
                    } else {
                        None
                    };
                    if let Some(message) = message {
                        let mut error =
                            Diagnostic::new(data.variant(variant).fields[index].range, message);
                        error.file = data.loc.file;
                        reported.push(error);
                    }
                }
            }
        }
    }
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        if imp.trait_ref.is_none() && imp.generics.predicates.is_empty() {
            continue;
        }
        let generics = generic_names(items, Some(Owner::Impl(id)), &Generics::default());
        let consts = generic_consts(items, Some(Owner::Impl(id)), &Generics::default());
        let unsized_params = unsized_params(items, Some(Owner::Impl(id)), &Generics::default());
        let self_ty = sigs.impls[id];
        let scope = TypeScope {
            self_ty: Some(self_ty),
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("implementations"),
            defaults: Some(&sigs.trait_defaults),
            struct_fields: Some(&sigs.structs),
            lens,
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let mut self_trait = None;
        let mut projections = Vec::new();
        let resolved =
            resolve_predicates(items, &sigs, imp.module, &imp.generics, scope, &mut errors);
        let bounds = traits::elaborate(&sigs, &resolved.predicates);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
        if let (Some(t), Some(bound)) = (imp.trait_, &imp.trait_ref)
            && let Some((trait_ref, bindings)) =
                traits::trait_ref(items, imp.module, t, bound, self_ty, scope, &mut errors)
        {
            sigs.impl_traits.insert(id, trait_ref);
            self_trait = Some(trait_ref);
            // `impl Tr<Out = i32> for S` says nothing: its block does.
            if !bindings.is_empty() {
                for args in bound.path.args.iter().flatten() {
                    for b in args.bindings.iter() {
                        errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
                    }
                }
            }
        }
        projections.extend(resolved.projections);
        let self_supertraits = traits::associated_self_bounds(&sigs, self_trait, Some(self_ty));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            bounds: &bounds,
            self_trait,
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            ..scope
        };
        let types: Box<[(Symbol, Ty)]> = imp
            .types
            .iter()
            .map(|t| {
                let ty = resolve_type(items, imp.module, &t.ty, scope, t.ty_range, &mut errors);
                (t.name, ty)
            })
            .collect();
        in_file(errors, imp.file, &mut reported);
        if !types.is_empty() {
            sigs.impl_types.insert(id, types);
        }
        if !resolved.predicates.is_empty() {
            sigs.impl_predicates.insert(id, resolved.predicates.into());
        }
        if !projections.is_empty() {
            sigs.impl_projections.insert(id, projections.into());
        }
    }
    for (id, konst) in items.consts.iter().skip(from.consts as usize) {
        // A discriminant has the enum's `repr` type.
        if let Some((strukt, _)) = konst.discriminant {
            sigs.consts
                .insert(id, Ty::int(discriminant_ty(items, strukt)));
            continue;
        }
        // A const generic argument has its parameter's type, known once
        // the path it is in is resolved.
        if konst.anon.is_some_and(|a| a.is_arg) {
            let ty = lens.and_then(|l| l.arg_ty(id)).unwrap_or(Ty::ERROR);
            sigs.consts.insert(id, ty);
            continue;
        }
        let generics = generic_names(items, konst.owner, &Generics::default());
        // Its owner's bounds, for `Self::A` or `T::A` in its type.
        let bounds = traits::elaborate(&sigs, &traits::owner_predicates(items, &sigs, konst.owner));
        let self_trait = konst
            .owner
            .and_then(Owner::as_impl)
            .and_then(|imp| sigs.impl_traits.get(imp).copied());
        let self_supertraits =
            traits::associated_self_bounds(&sigs, self_trait, self_ty(&sigs, konst.owner));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, konst.owner),
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            generics: &generics,
            consts: &[],
            signature_of: Some(match (konst.owner, konst.is_static) {
                (Some(_), _) => "associated constants",
                (None, Some(_)) => "static variables",
                (None, None) => "constants",
            }),
            bounds: &bounds,
            in_fn_pointer: false,
            in_trait_bound: false,
            self_trait: konst
                .owner
                .and_then(Owner::as_impl)
                .and_then(|imp| sigs.impl_traits.get(imp).copied()),
            self_types: &[],
            defaults: Some(&sigs.trait_defaults),
            struct_fields: Some(&sigs.structs),
            lens,
            function: None,
            unsized_params: &[],
        };
        let mut ty = resolve(
            items,
            konst.loc,
            &konst.ty,
            scope,
            konst.ty_range,
            &mut reported,
        );
        if scope.is_unsized(items, ty) {
            let mut d = Diagnostic::new(konst.ty_range, unsized_error(ty));
            d.file = konst.loc.file;
            reported.push(d);
            ty = Ty::ERROR;
        }
        if let Some(mutable) = konst.is_static {
            let mut error = |message: String| {
                reported.push(Diagnostic {
                    file: konst.loc.file,
                    range: konst.ty_range,
                    message,
                });
            };
            fn mutable_ref(ty: Ty) -> bool {
                matches!(ty.kind(), TyKind::Ref(Mutability::Mut, _))
                    || ty.children().any(mutable_ref)
            }
            if mutable_ref(ty) {
                error("mutable references in statics are not supported yet".to_owned());
            } else if !mutable && let Some(pointer) = not_sync(items, &sigs, ty, &mut Vec::new()) {
                // Name the innermost failing type, including a trait
                // object's lifetime bound as rustc does.
                let name = if pointer.is_dyn() {
                    format!("({pointer} + 'static)")
                } else {
                    pointer.to_string()
                };
                error(format!("`{name}` cannot be shared between threads safely"));
            }
        }
        sigs.consts.insert(id, ty);
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let generics = generic_names(items, function.owner, &function.generics);
        let consts = generic_consts(items, function.owner, &function.generics);
        let unsized_params = unsized_params(items, function.owner, &function.generics);
        let mut errors = Vec::new();
        check_const_params(
            items,
            function.loc.module,
            &function.generics,
            &generics,
            &mut errors,
        );
        in_file(errors, function.loc.file, &mut reported);
        let self_trait = items
            .impl_of(id)
            .and_then(|imp| sigs.impl_traits.get(imp).copied());
        let owner_bounds = traits::owner_predicates(items, &sigs, function.owner);
        let self_types: Vec<(Symbol, Ty)> = match items.impl_of(id) {
            Some(imp) if self_trait.is_some() => {
                sigs.impl_types.get(imp).map_or(Vec::new(), |t| t.to_vec())
            }
            _ => Vec::new(),
        };
        let self_supertraits =
            traits::associated_self_bounds(&sigs, self_trait, self_ty(&sigs, function.owner));
        let self_inherited_types = traits::associated_self_types(items, &sigs, &self_supertraits);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, function.owner),
            self_supertraits: &self_supertraits,
            self_inherited_types: &self_inherited_types,
            generics: &generics,
            consts: &consts,
            unsized_params: &unsized_params,
            signature_of: Some("functions"),
            self_trait,
            self_types: &self_types,
            defaults: Some(&sigs.trait_defaults),
            struct_fields: Some(&sigs.structs),
            lens,
            ..TypeScope::default()
        };
        // Its bounds first, then its types, in which `T::Out` may name
        // their associated types.
        let mut own = traits::Bounds::default();
        if !function.generics.predicates.is_empty() {
            let elaborated = traits::elaborate(&sigs, &owner_bounds);
            let scope = TypeScope {
                bounds: &elaborated,
                ..scope
            };
            let mut errors = Vec::new();
            own = resolve_predicates(
                items,
                &sigs,
                function.loc.module,
                &function.generics,
                scope,
                &mut errors,
            );
            in_file(errors, function.loc.file, &mut reported);
        }
        let all: Vec<Predicate> = owner_bounds
            .iter()
            .chain(&own.predicates)
            .copied()
            .collect();
        let bounds = traits::elaborate(&sigs, &all);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
        // Parameters and return values must be sized; a declaration
        // without a body may name a trait's `Self` or a `?Sized`
        // parameter, as rustc checks only bodies' (and not where a bound
        // makes the type `Sized`).
        let has_body = function.has_body;
        let sized = |ty: Ty, range: TextRange, reported: &mut Vec<Diagnostic>| {
            // A DST tuple struct has a constructor item but cannot be built
            // by value. Calls are rejected by the ordinary value checks.
            if function.ctor.is_some() {
                return ty;
            }
            // A parameter keeps its type, which the body's bindings have.
            if matches!(ty.kind(), TyKind::Param(_)) {
                if has_body && scope.is_unsized(items, ty) {
                    let mut d = Diagnostic::new(range, unsized_error(ty));
                    d.file = function.loc.file;
                    reported.push(d);
                }
                return ty;
            }
            if scope.is_unsized(items, ty) {
                // A trait object here has its default bound, `'static`,
                // which rustc names.
                let message = if ty.is_dyn() {
                    format!(
                        "the size for values of type `({ty} + 'static)` cannot be known at compilation time"
                    )
                } else {
                    unsized_error(ty)
                };
                let mut d = Diagnostic::new(range, message);
                d.file = function.loc.file;
                reported.push(d);
                return Ty::ERROR;
            }
            ty
        };
        let params: Box<[Ty]> = function
            .params
            .iter()
            .map(|p| {
                let ty = resolve(items, function.loc, &p.ty, scope, p.range, &mut reported);
                sized(ty, p.range, &mut reported)
            })
            .collect();
        if function.has_self
            && let (Some(&receiver), Some(self_ty)) = (params.first(), scope.self_ty)
        {
            let receiver = traits::normalize(items, &sigs, &[], receiver);
            let valid = receiver == self_ty
                || matches!(receiver.kind(), TyKind::Ref(_, pointee) if pointee == self_ty)
                || super::box_pointee(receiver) == Some(self_ty);
            if !receiver.has_error() && !valid {
                let mut diagnostic = Diagnostic::new(
                    function.params[0].range,
                    "this receiver type is not supported yet; expected Self, &Self, &mut Self or Box<Self>",
                );
                diagnostic.file = function.loc.file;
                reported.push(diagnostic);
            }
        }
        let ret = resolve(
            items,
            function.loc,
            &function.ret,
            TypeScope {
                function: Some(id),
                ..scope
            },
            function.ret_range,
            &mut reported,
        );
        let ret = sized(ret, function.ret_range, &mut reported);
        sigs.fns.insert(id, FnSig { params, ret });
        if !function.opaques.is_empty() {
            let mut errors = Vec::new();
            let opaques = opaque_sigs(items, function, scope, &mut errors);
            in_file(errors, function.loc.file, &mut reported);
            sigs.opaques.insert(id, opaques);
        }
        if !own.predicates.is_empty() {
            sigs.fn_predicates.insert(id, own.predicates.into());
        }
        if !own.projections.is_empty() {
            sigs.fn_projections.insert(id, own.projections.into());
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        // A parameter must be a field's type, of some variant for an
        // enum, except `PhantomData`'s, which acts as if it were one.
        if is_lang_struct(items, id, "phantom_data") {
            continue;
        }
        for (i, param) in s.generics.params.iter().enumerate() {
            let fields = sigs.structs[id];
            // Const parameters need not be used, as in rustc.
            if param.is_const() {
                continue;
            }
            if !fields.iter().any(|&f| uses_param(f, i as u32)) && !fields.contains(&Ty::ERROR) {
                error(
                    s.loc.file,
                    param.range,
                    &format!("type parameter `{}` is never used", param.name),
                );
            }
        }
    }
    for (id, alias) in items.type_aliases.iter().skip(from.type_aliases as usize) {
        check_alias(items, id, alias, lens, &sigs, &mut reported);
    }
    regions::resolve_regions(items, &mut sigs, from, &mut reported);
    traits::check_well_formed(items, &sigs, from, &mut reported);
    duplicate_assoc_items(items, &sigs, &mut reported, from.impls as usize);
    traits::check_impls(items, &sigs, &mut reported, checked_impls);
    // Union fields are checked after all signatures and impl predicates exist:
    // a concrete generic struct's Copy impl may depend on its arguments.
    for (id, data) in items.structs.iter().skip(from.structs as usize) {
        if !data.is_union {
            continue;
        }
        for (field, &ty) in data.fields.iter().zip(sigs.structs[id]) {
            if ty.has_error() {
                continue;
            }
            let ty = traits::normalize(items, &sigs, &[], ty);
            let message = if !supported_union_field(items, &sigs, ty, &mut FxHashMap::default(), 0)
            {
                Some("this union field type is not supported yet")
            } else if !traits::is_copy(items, &sigs, &[], ty)
                || traits::needs_drop(items, &sigs, &[], ty)
            {
                Some("union fields must implement Copy")
            } else {
                None
            };
            if let Some(message) = message {
                let mut d = Diagnostic::new(field.range, message);
                d.file = data.loc.file;
                reported.push(d);
            }
        }
    }
    sigs.cyclic |= traits::check_supertrait_cycles(items, &sigs, &mut reported, from);
    traits::check_trivial_bounds(items, &sigs, &mut reported, from);
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let ty = sigs.impls[id];
        let trait_args = sigs.impl_traits.get(id).map_or(&[][..], |t| t.args);
        // As in rustc, a parameter an associated type of constrained ones
        // is bound to is constrained too.
        let count = imp.generics.params.len();
        let mut constrained: Vec<bool> = (0..count as u32)
            .map(|i| mentions_param(ty, i) || trait_args.iter().any(|&a| mentions_param(a, i)))
            .collect();
        loop {
            let mut progress = false;
            for p in sigs.impl_projections.get(id).into_iter().flatten() {
                let inputs = (0..count as u32)
                    .filter(|&i| p.alias.args.iter().any(|&a| mentions_param(a, i)))
                    .all(|i| constrained[i as usize]);
                if !inputs {
                    continue;
                }
                for (i, c) in constrained.iter_mut().enumerate() {
                    if !*c && mentions_param(p.ty, i as u32) {
                        *c = true;
                        progress = true;
                    }
                }
            }
            if !progress {
                break;
            }
        }
        for (i, param) in imp.generics.params.iter().enumerate() {
            let constrained = constrained[i];
            if ty != Ty::ERROR && !constrained {
                error(
                    imp.file,
                    param.range,
                    &format!(
                        "the type parameter `{}` is not constrained by the impl trait, self type, or predicates",
                        param.name
                    ),
                );
            }
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        let ty = Ty::adt(id, s.name, &params_of(&s.generics, 0));
        if contains(&sigs, ty, id, &mut Vec::new()) {
            error(
                s.loc.file,
                s.def_range,
                &format!("recursive type `{}` has infinite size", s.name),
            );
        }
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let sig = &sigs.fns[id];
        let params = function.params.iter().zip(&sig.params);
        if function.c_abi {
            for (param, &ty) in params.clone() {
                if contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "passing references to C is not supported yet",
                    );
                }
            }
            // Aggregate ABI layouts are checked after substitution by codegen,
            // covering generic definitions and C function pointers alike.
            let types = params
                .map(|(p, &ty)| (ty, p.range))
                .chain([(sig.ret, function.ret_range)]);
            for (ty, range) in types {
                let sequence = match ty.kind() {
                    TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => t.is_unsized(),
                    _ => false,
                };
                if sequence {
                    error(
                        function.loc.file,
                        range,
                        "passing slices to C is not supported yet",
                    );
                }
            }
        }
    }
    diagnostics.extend(reported);
    regions::check_trait_admission(items, &sigs, from, &mut diagnostics);
    (sigs, diagnostics)
}
