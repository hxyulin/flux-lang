//! Generic parameters, bounds and trusted Drop promises as written.
use super::{
    AstChildren, AstNode, Defaults, Diagnostic, GenericParam, Generics, IN_BOUNDS, ImplTraitSite,
    LifetimeParam, OutlivesPredicate, ParamKind, Path, Relaxed, Symbol, TextRange, TypeOutlives,
    TypeRef, WherePredicate, apply_bound_binder, ast, attr_name, binder_names, check_attr,
    check_attrs, expanded_type, lower_array_len, lower_bounds, lower_const_arg, lower_dyn,
    lower_fn_ptr, lower_generic_path, lower_impl_trait, lower_lifetime, reject_impl_trait,
    reject_impl_trait_in_path, unsupported,
};

/// An item's generic parameter list and `where` clause: lifetime and
/// type parameters, type parameters without defaults, with distinct
/// names, trait bounds on types and lifetime bounds.
pub(super) fn collect_generics(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults: Defaults,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    collect_generics_with_drop(list, where_clause, defaults, false, diagnostics)
}

pub(super) fn collect_generics_with_drop(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults: Defaults,
    trusted_drop: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    let mut generics =
        collect_generics_as_written(list, where_clause, defaults, trusted_drop, diagnostics);
    let bounds = ImplTraitSite::NotAllowed("bounds");
    for p in generics.predicates.iter_mut() {
        reject_impl_trait(&mut p.ty, bounds, diagnostics);
        reject_impl_trait_in_path(&mut p.bound.path, IN_BOUNDS, diagnostics);
    }
    for p in generics.type_outlives.iter_mut() {
        reject_impl_trait(&mut p.ty, bounds, diagnostics);
    }
    for p in generics.params.iter_mut() {
        if let Some((ty, _)) = &mut p.default {
            let site = ImplTraitSite::NotAllowed("generic parameter defaults");
            reject_impl_trait(ty, site, diagnostics);
        }
    }
    generics
}

/// Only embedded implementation parameters can carry the internal unsafe
/// drop promise. Merely naming a user crate core/alloc/std grants no privilege.
fn drop_param_attrs(
    attrs: AstChildren<'_, ast::Attr<'_>>,
    trusted: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let attrs = crate::cfg::Attrs::new(attrs);
    diagnostics.extend(
        attrs
            .errors
            .iter()
            .map(|(r, m)| Diagnostic::new(*r, m.clone())),
    );
    let mut found = false;
    for attr in attrs.iter() {
        if trusted && attr_name(attr) == Some("may_dangle") {
            let valid = attr.meta().is_some_and(|meta| {
                meta.token_tree().is_none()
                    && meta.eq_token().is_none()
                    && meta.unsafe_token().is_none()
            });
            if !valid || found {
                diagnostics.push(Diagnostic::new(
                    attr.syntax().text_range(),
                    "invalid `may_dangle` attribute",
                ));
            }
            found |= valid;
        } else {
            check_attr(attr, diagnostics);
        }
    }
    found
}

fn collect_generics_as_written(
    list: Option<ast::GenericParamList<'_>>,
    where_clause: Option<ast::WhereClause<'_>>,
    defaults: Defaults,
    trusted_drop: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Generics {
    let mut predicates = Vec::new();
    let mut outlives = Vec::new();
    let mut type_outlives = Vec::new();
    // `?Sized` bounds: on what type, and where each is written.
    let mut relaxed: Vec<(TypeRef, Vec<TextRange>)> = Vec::new();
    let where_predicates = |predicates: &mut Vec<WherePredicate>,
                            outlives: &mut Vec<OutlivesPredicate>,
                            type_outlives: &mut Vec<TypeOutlives>,
                            relaxed: &mut Vec<(TypeRef, Vec<TextRange>)>,
                            diagnostics: &mut Vec<Diagnostic>| {
        let Some(clause) = where_clause else {
            return;
        };
        for pred in clause.predicates() {
            let range = pred.syntax().text_range();
            // `where for<'a> F: Fn(&'a T) -> &'a U`, as the bound's own.
            let binder = pred
                .for_token()
                .map(|_| binder_names(pred.generic_param_list(), diagnostics));
            if let Some(None) = binder {
                unsupported(diagnostics, range, "higher-ranked bounds");
                continue;
            }
            let mut bounds = Vec::new();
            let mut lifetimes = Vec::new();
            if let Some(lifetime) = pred.lifetime() {
                // `'a: 'b + 'c`
                let longer = lower_lifetime(lifetime);
                if let Some(list) = pred.type_bound_list() {
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut lifetimes),
                        Relaxed::Unsupported,
                        diagnostics,
                    );
                }
                for bound in bounds {
                    diagnostics.push(Diagnostic::new(
                        bound.range,
                        "expected a lifetime, found a trait",
                    ));
                }
                outlives.extend(
                    lifetimes
                        .into_iter()
                        .map(|shorter| OutlivesPredicate { longer, shorter }),
                );
                continue;
            }
            let ty_range = pred.ty().map_or(range, |t| t.syntax().text_range());
            let ty = lower_type(pred.ty(), diagnostics);
            let mut relaxed_here = Vec::new();
            if let Some(list) = pred.type_bound_list() {
                lower_bounds(
                    list,
                    &mut bounds,
                    Some(&mut lifetimes),
                    Relaxed::Collect(&mut relaxed_here),
                    diagnostics,
                );
            }
            let has_relaxed = !relaxed_here.is_empty();
            if has_relaxed {
                relaxed.push((ty.clone(), relaxed_here));
            }
            if let Some(Some(names)) = &binder {
                if !lifetimes.is_empty() || has_relaxed {
                    unsupported(
                        diagnostics,
                        range,
                        "higher-ranked outlives and relaxed bounds",
                    );
                    continue;
                }
                if !bounds
                    .iter_mut()
                    .all(|bound| apply_bound_binder(bound, names, diagnostics))
                {
                    continue;
                }
            }
            predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                ty: ty.clone(),
                ty_range,
                bound,
            }));
            type_outlives.extend(lifetimes.into_iter().map(|lifetime| TypeOutlives {
                ty: ty.clone(),
                ty_range,
                lifetime,
            }));
        }
    };
    let Some(list) = list else {
        where_predicates(
            &mut predicates,
            &mut outlives,
            &mut type_outlives,
            &mut relaxed,
            diagnostics,
        );
        expand_associated_bounds(&mut predicates, &mut type_outlives, diagnostics);
        mark_relaxed(&mut [], relaxed, diagnostics);
        return Generics {
            predicates: predicates.into(),
            outlives: outlives.into(),
            type_outlives: type_outlives.into(),
            ..Generics::default()
        };
    };
    let mut params: Vec<GenericParam> = Vec::new();
    let mut lifetimes: Vec<LifetimeParam> = Vec::new();
    // Parameters with defaults so far, which no parameter without one
    // may follow.
    let mut defaulted: Vec<TextRange> = Vec::new();
    let already_used = |name: Symbol| {
        format!(
            "the name `{name}` is already used for a generic parameter in this item's generic parameters"
        )
    };
    for param in list.generic_params() {
        let range = param.syntax().text_range();
        match param {
            ast::GenericParam::TypeParam(param) => {
                let may_dangle = drop_param_attrs(param.attrs(), trusted_drop, diagnostics);
                let Some(name) = param.name() else {
                    continue;
                };
                if let Some(list) = param.type_bound_list() {
                    let mut bounds = Vec::new();
                    let mut bound_lifetimes = Vec::new();
                    let mut relaxed_here = Vec::new();
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut bound_lifetimes),
                        Relaxed::Collect(&mut relaxed_here),
                        diagnostics,
                    );
                    let ty = TypeRef::Path(Path::single(Symbol::intern(name.text())));
                    if !relaxed_here.is_empty() {
                        relaxed.push((ty.clone(), relaxed_here));
                    }
                    let ty_range = name.syntax().text_range();
                    predicates.extend(bounds.into_iter().map(|bound| WherePredicate {
                        ty: ty.clone(),
                        ty_range,
                        bound,
                    }));
                    type_outlives.extend(bound_lifetimes.into_iter().map(|lifetime| {
                        TypeOutlives {
                            ty: ty.clone(),
                            ty_range,
                            lifetime,
                        }
                    }));
                }
                let name_range = name.syntax().text_range();
                let mut default = None;
                if let Some(ty) = param.default_type() {
                    let ty_range = ty.syntax().text_range();
                    match defaults {
                        Defaults::Trait | Defaults::Struct | Defaults::Alias => {
                            default = Some((lower_type(Some(ty), diagnostics), ty_range));
                        }
                        Defaults::No => diagnostics.push(Diagnostic::new(
                            range,
                            "defaults for generic parameters are not allowed here",
                        )),
                    }
                    if defaults != Defaults::No {
                        defaulted.push(name_range);
                    }
                } else if defaults != Defaults::No
                    && let Some(&earlier) = defaulted.first()
                {
                    diagnostics.push(Diagnostic::new(
                        earlier,
                        "generic parameters with a default must be trailing",
                    ));
                    defaulted.clear();
                }
                let name = Symbol::intern(name.text());
                if params.iter().any(|p| p.name == name) {
                    diagnostics.push(Diagnostic::new(name_range, already_used(name)));
                    continue;
                }
                params.push(GenericParam {
                    name,
                    range: name_range,
                    default,
                    synthetic: false,
                    kind: ParamKind::Type,
                    maybe_unsized: false,
                    may_dangle,
                });
                continue;
            }
            ast::GenericParam::ConstParam(param) => {
                check_attrs(param.attrs(), diagnostics);
                let Some(name) = param.name() else {
                    continue;
                };
                let name_range = name.syntax().text_range();
                let ty_range = param.ty().map_or(range, |t| t.syntax().text_range());
                let ty = lower_type(param.ty(), diagnostics);
                let mut default = None;
                if let Some(value) = param.default_val() {
                    let at = value.syntax().text_range();
                    match defaults {
                        Defaults::No => diagnostics.push(Diagnostic::new(
                            at,
                            "defaults for generic parameters are not allowed here",
                        )),
                        _ => {
                            default = Some((
                                TypeRef::Const(lower_const_arg(value.expr(), at, diagnostics)),
                                at,
                            ));
                            defaulted.push(name_range);
                        }
                    }
                } else if defaults != Defaults::No
                    && let Some(&earlier) = defaulted.first()
                {
                    diagnostics.push(Diagnostic::new(
                        earlier,
                        "generic parameters with a default must be trailing",
                    ));
                    defaulted.clear();
                }
                let name = Symbol::intern(name.text());
                if params.iter().any(|p| p.name == name) {
                    diagnostics.push(Diagnostic::new(name_range, already_used(name)));
                    continue;
                }
                params.push(GenericParam {
                    name,
                    range: name_range,
                    default,
                    synthetic: false,
                    kind: ParamKind::Const { ty, ty_range },
                    maybe_unsized: false,
                    may_dangle: false,
                });
                continue;
            }
            ast::GenericParam::LifetimeParam(param) => {
                let may_dangle = drop_param_attrs(param.attrs(), trusted_drop, diagnostics);
                let Some(lifetime) = param.lifetime() else {
                    continue;
                };
                let lifetime = lower_lifetime(lifetime);
                if !params.is_empty() {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "lifetime parameters must be declared prior to type and const parameters",
                    ));
                }
                if let Some(list) = param.type_bound_list() {
                    let mut bounds = Vec::new();
                    let mut shorter = Vec::new();
                    lower_bounds(
                        list,
                        &mut bounds,
                        Some(&mut shorter),
                        Relaxed::Unsupported,
                        diagnostics,
                    );
                    for bound in bounds {
                        diagnostics.push(Diagnostic::new(
                            bound.range,
                            "expected a lifetime, found a trait",
                        ));
                    }
                    outlives.extend(shorter.into_iter().map(|shorter| OutlivesPredicate {
                        longer: lifetime,
                        shorter,
                    }));
                }
                if lifetime.is_static() {
                    diagnostics.push(Diagnostic::new(
                        lifetime.range,
                        "invalid lifetime parameter name: `'static`",
                    ));
                    continue;
                }
                if lifetime.is_anonymous() {
                    diagnostics.push(Diagnostic::new(lifetime.range, "`'_` cannot be used here"));
                    continue;
                }
                if lifetimes.iter().any(|l| l.name == lifetime.name) {
                    diagnostics.push(Diagnostic::new(lifetime.range, already_used(lifetime.name)));
                    continue;
                }
                lifetimes.push(LifetimeParam {
                    may_dangle,
                    name: lifetime.name,
                    range: lifetime.range,
                });
                continue;
            }
        }
    }
    where_predicates(
        &mut predicates,
        &mut outlives,
        &mut type_outlives,
        &mut relaxed,
        diagnostics,
    );
    expand_associated_bounds(&mut predicates, &mut type_outlives, diagnostics);
    mark_relaxed(&mut params, relaxed, diagnostics);
    Generics {
        params: params.into(),
        lifetimes: lifetimes.into(),
        predicates: predicates.into(),
        outlives: outlives.into(),
        type_outlives: type_outlives.into(),
        range: list.syntax().text_range(),
    }
}

/// Marks the parameters `?Sized` bounds are on, which must be the
/// item's own type parameters, each relaxed once (E0203).
fn mark_relaxed(
    params: &mut [GenericParam],
    relaxed: Vec<(TypeRef, Vec<TextRange>)>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let mut seen: Vec<(Symbol, TextRange)> = Vec::new();
    for (ty, ranges) in relaxed {
        let param = match &ty {
            TypeRef::Path(path) => path
                .as_name()
                .and_then(|name| params.iter_mut().find(|p| p.name == name && !p.is_const())),
            _ => None,
        };
        let Some(param) = param else {
            for range in ranges {
                diagnostics.push(Diagnostic::new(
                    range,
                    "this relaxed bound is not permitted here",
                ));
            }
            continue;
        };
        param.maybe_unsized = true;
        for range in ranges {
            if let Some(&(_, first)) = seen.iter().find(|(n, _)| *n == param.name) {
                diagnostics.push(Diagnostic::new(first, "duplicate relaxed `Sized` bounds"));
            } else {
                seen.push((param.name, range));
            }
        }
    }
}

/// Lowers a type as written. A missing type was already reported by the
/// parser.
pub(crate) fn lower_type(ty: Option<ast::Type<'_>>, diagnostics: &mut Vec<Diagnostic>) -> TypeRef {
    let Some(ty) = ty else {
        return TypeRef::Error;
    };
    let what = match ty {
        ast::Type::PathType(path) => {
            return match path.path().and_then(|p| lower_generic_path(p, diagnostics)) {
                Some(path) => TypeRef::Path(path),
                None => TypeRef::Error,
            };
        }
        ast::Type::TupleType(tuple) => {
            let fields = tuple
                .fields()
                .map(|field| lower_type(Some(field), diagnostics))
                .collect();
            return TypeRef::Tuple(fields);
        }
        ast::Type::ParenType(paren) => return lower_type(paren.ty(), diagnostics),
        ast::Type::NeverType(_) => return TypeRef::Never,
        ast::Type::ArrayType(array) => {
            let elem = lower_type(array.ty(), diagnostics);
            let len = lower_array_len(array.expr(), ty.syntax().text_range(), diagnostics);
            return TypeRef::Array {
                elem: Box::new(elem),
                len,
            };
        }
        ast::Type::DynTraitType(d) => return lower_dyn(d, diagnostics),
        ast::Type::FnPtrType(ptr) => return lower_fn_ptr(ptr, diagnostics),
        ast::Type::ForType(for_type) => {
            let Some(names) = binder_names(for_type.generic_param_list(), diagnostics) else {
                return TypeRef::Error;
            };
            let mut ty = lower_type(for_type.ty(), diagnostics);
            if let TypeRef::FnPtr {
                bound_lifetimes, ..
            } = &mut ty
            {
                if !bound_lifetimes.is_empty() {
                    unsupported(
                        diagnostics,
                        for_type.syntax().text_range(),
                        "nested function pointer binders",
                    );
                    return TypeRef::Error;
                }
                *bound_lifetimes = names.into();
                return ty;
            }
            "higher-ranked types"
        }
        ast::Type::ImplTraitType(it) => return lower_impl_trait(it, diagnostics),
        ast::Type::InferType(_) => return TypeRef::Infer,
        ast::Type::MacroType(m) => {
            if let Some(ty) = expanded_type(m.syntax()) {
                return ty;
            }
            "macros in this position"
        }
        ast::Type::PtrType(ptr) => {
            let pointee = lower_type(ptr.ty(), diagnostics);
            return TypeRef::Ptr {
                mutable: ptr.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::RefType(r) => {
            let pointee = lower_type(r.ty(), diagnostics);
            return TypeRef::Ref {
                lifetime: r.lifetime().map(lower_lifetime),
                amp: r
                    .amp_token()
                    .map_or(ty.syntax().text_range(), |t| t.text_range()),
                mutable: r.mut_token().is_some(),
                pointee: Box::new(pointee),
            };
        }
        ast::Type::SliceType(slice) => {
            return TypeRef::Slice(Box::new(lower_type(slice.ty(), diagnostics)));
        }
    };
    unsupported(diagnostics, ty.syntax().text_range(), what);
    TypeRef::Error
}

fn expand_associated_bounds(
    predicates: &mut Vec<WherePredicate>,
    outlives: &mut Vec<TypeOutlives>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    fn expand(
        mut predicate: WherePredicate,
        output: &mut Vec<WherePredicate>,
        outlives: &mut Vec<TypeOutlives>,
        diagnostics: &mut Vec<Diagnostic>,
        depth: usize,
    ) {
        if depth > 64 {
            diagnostics.push(Diagnostic::new(
                predicate.bound.range,
                "associated type bounds are nested too deeply",
            ));
            return;
        }
        let constraints = predicate
            .bound
            .path
            .args
            .last_mut()
            .and_then(Option::as_mut)
            .map(|args| std::mem::take(&mut args.bounds))
            .unwrap_or_default();
        output.push(predicate.clone());
        for constraint in constraints {
            if constraint.bounds.is_empty() && constraint.lifetimes.is_empty() {
                diagnostics.push(Diagnostic::new(
                    constraint.range,
                    "empty associated type bounds are not supported yet",
                ));
                continue;
            }
            if constraint.args.is_some() || !predicate.bound.bound_lifetimes.is_empty() {
                diagnostics.push(Diagnostic::new(
                    constraint.range,
                    "associated family and higher-ranked output bounds are not supported yet",
                ));
                continue;
            }
            let mut path = Path::single(constraint.name);
            path.qself = Some(Box::new(super::QSelf {
                associated_bound: true,
                ty: predicate.ty.clone(),
                ty_range: predicate.ty_range,
                trait_: Some(predicate.bound.path.clone()),
                range: constraint.range,
            }));
            let ty = TypeRef::Path(path);
            for bound in constraint.bounds {
                expand(
                    WherePredicate {
                        ty: ty.clone(),
                        ty_range: constraint.range,
                        bound,
                    },
                    output,
                    outlives,
                    diagnostics,
                    depth + 1,
                );
            }
            outlives.extend(
                constraint
                    .lifetimes
                    .into_iter()
                    .map(|lifetime| TypeOutlives {
                        ty: ty.clone(),
                        ty_range: constraint.range,
                        lifetime,
                    }),
            );
        }
    }
    for predicate in std::mem::take(predicates) {
        expand(predicate, predicates, outlives, diagnostics, 0);
    }
}
