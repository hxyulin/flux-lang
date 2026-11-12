//! Type inference and checking.
//!
//! Signatures come first, from the item tree alone ([`signatures`]); then
//! each body is inferred on its own against them ([`infer_body`]), so
//! bodies can be checked in parallel and an edit to one body does not
//! affect another's types.
//!
//! Inference is unification over type variables. An unsuffixed integer
//! literal gets an *integer* variable that only unifies with integer types
//! and defaults to `i32`. `!` coerces to any type instead of unifying, and
//! a variable fed only by `!` falls back to `!`, as in edition 2024.
//! Checks that need final types (literal ranges, operand types, casts)
//! run once the body is inferred.
//!
//! This is also where flux makes the checks rustc makes before borrow
//! checking: calls to `unsafe` functions need an `unsafe` block, and
//! assignments need a `mut` binding.

pub mod const_dependencies;
pub mod entry;
pub mod flib;
pub mod inline_asm;
mod signature;
pub mod sized_dispatch;
pub use signature::{signatures, signatures_checked, signatures_from};

mod resolve;
pub use resolve::{
    ArrayLens, LenEvaluator, TypeScope, alias_type, array_len, check_const_params, const_param_ty,
    const_value, generic_consts, generic_names, impl_target, nonnull_pointee, nonzero_integer,
    param_const_ty, primitive, resolve_generic_arg, resolve_generic_args, resolve_type, self_ty,
    shared_pointee, shared_pointees, struct_type, unsized_error, wrong_arg_count,
};
use resolve::{
    alias_cycle, check_type_defaults, reaches_alias, resolve_or_report, resolve_predicates,
    unsized_argument,
};
pub(crate) use resolve::{contains_ref, expand_alias, fill_type_defaults, type_arg_count};

mod fn_regions;
mod infer;
mod library;
pub use library::LibraryTypes;
pub mod objects;
mod pats;
pub mod regions;
pub mod traits;

use std::fmt::Write;
use std::sync::OnceLock;

use flux_arena::{ArenaMap, LayeredArenaMap};
use flux_hir::def_map::{Def, Ns, ResolveError, Resolved};
use flux_hir::{
    Const, ConstId, Core, Diagnostic, Function, FunctionId, Generics, Hir, Impl, ImplId,
    ImplTarget, ItemCounts, ItemLoc, ItemTree, ModuleId, Owner, Path, Struct, StructId, TypeRef,
};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::TextRange;
use rustc_hash::FxHashMap;

pub use flux_ty_ir::{
    AdtId, AliasTy, ArgTys, Builtins, ClosureArgs, ClosureId, ClosureKind, ClosureOwner,
    ConstInstance, DynTy, FloatTy, FnDefTy, FnProjection, FnPtrRegions, FnRegion, FnSig, FnSigTy,
    GenericArgs, InferVar, Instance, IntTy, LenValues, Metadata, Mutability, OpaqueSig, OpaqueTy,
    ParamTy, Signatures, Ty, TyKind, fn_trait, fn_trait_kind, params_of, subst_list, symbol_list,
};
use flux_ty_ir::{impl_index, in_scope, mentions_param, metadata, overlap, ty};
pub use infer::{
    BuiltinMethod, Capture, CaptureMode, CaptureProj, ClosureInfo, InferenceResult, Method,
    OverloadedDeref, c_char_ptr, c_str_ref, infer_body, infer_const,
};
pub use traits::{Predicate, Projection, TraitRef, TraitSig};

/// Reports names defined twice by blocks for overlapping instances of a
/// generic struct, `impl<T> S<T>` and `impl S<i32>`; the item tree
/// checked other types, and blocks for one generic struct that cannot
/// overlap (`impl S<u8>` and `impl S<i32>`) may share names. Only pairs
/// whose later block is from `from` on are checked.
fn duplicate_assoc_items(
    items: &ItemTree,
    sigs: &Signatures,
    diagnostics: &mut Vec<Diagnostic>,
    from: usize,
) {
    let generic: Vec<(ImplId, &Impl)> = items
        .impls
        .iter()
        .filter(|(_, imp)| match imp.target {
            Some(ImplTarget::Struct(s)) => !items.structs[s].generics.is_empty(),
            Some(ImplTarget::Object(_)) => true,
            _ => false,
        })
        .collect();
    let names = |imp: &Impl| -> Vec<(Symbol, TextRange)> {
        let functions = imp.functions.iter().map(|&f| {
            let f = &items.functions[f];
            (f.name, f.def_range)
        });
        let consts = imp.consts.iter().filter_map(|&c| {
            let c = &items.consts[c];
            Some((c.name?, c.def_range))
        });
        functions.chain(consts).collect()
    };
    for (i, &(a, first)) in generic.iter().enumerate() {
        for &(b, second) in &generic[i + 1..] {
            if b.index() < from
                || first.target != second.target
                || !overlap(sigs.impls[a], sigs.impls[b])
            {
                continue;
            }
            let later = names(second);
            // rustc reports the earlier definition.
            for (name, range) in names(first) {
                if later.iter().any(|&(n, _)| n == name) {
                    diagnostics.push(Diagnostic {
                        file: first.file,
                        range,
                        message: format!("duplicate definitions with name `{name}`"),
                    });
                }
            }
        }
    }
}

/// Whether `ty` uses generic parameter `index`, through associated types
/// (`T::Out`) too, as a struct's fields may.
fn uses_param(ty: Ty, index: u32) -> bool {
    match ty.kind() {
        TyKind::Alias(alias) => alias
            .args
            .iter()
            .chain(alias.family_args)
            .any(|&t| uses_param(t, index)),
        TyKind::Opaque(opaque) => opaque.args.iter().any(|&t| uses_param(t, index)),
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Slice(t) => uses_param(t, index),
        TyKind::Array(t, len) => uses_param(t, index) || mentions_param(len, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().any(|&t| uses_param(t, index))
        }
        TyKind::FnPtr(_) => ty.children().any(|t| uses_param(t, index)),
        _ => mentions_param(ty, index),
    }
}

/// The parameters of `generics` as types, numbered from `first`.
/// The bounds of `function`'s opaque types, resolved in `scope`, its
/// signature's: each in terms of the function's generic parameters and,
/// for the opaque type itself, one after them.
fn opaque_sigs(
    items: &ItemTree,
    function: &flux_hir::Function,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Box<[OpaqueSig]> {
    let module = function.loc.module;
    let count = scope.generics.len() as u32;
    let mut out = Vec::new();
    for decl in function.opaques.iter() {
        let this = Ty::param(count, decl.name);
        let mut predicates = Vec::new();
        let mut projections = Vec::new();
        let mut bound_origins = Vec::new();
        for (origin, b) in decl.bounds.iter().enumerate() {
            if let Some((trait_ref, bindings)) =
                traits::resolve_bound(items, module, b, this, scope, errors)
            {
                bound_origins.push(origin as u32);
                predicates.push(Predicate {
                    self_ty: this,
                    trait_ref,
                });
                projections.extend(bindings);
            }
        }
        // `use<..>` must list every type parameter in scope, as rustc
        // requires so far.
        if let Some(captures) = &decl.captures {
            let listed = |name: Symbol| {
                captures
                    .iter()
                    .any(|c| matches!(c, flux_hir::UseCapture::Param(n, _) if *n == name))
            };
            let generics = items
                .owner_generics(function.owner)
                .into_iter()
                .chain([&function.generics]);
            if generics
                .flat_map(|g| g.params.iter())
                .any(|p| p.synthetic || !listed(p.name))
            {
                errors.push((
                    decl.range,
                    "`impl Trait` must mention all type parameters in scope in `use<...>`"
                        .to_owned(),
                ));
            }
            for c in captures.iter() {
                if let flux_hir::UseCapture::Param(name, range) = *c
                    && scope.param(name).is_none()
                    && name != kw::SelfUpper
                {
                    errors.push((range, format!("cannot find type `{name}` in this scope")));
                }
            }
        }
        out.push(OpaqueSig {
            bound_origins: bound_origins.into(),
            predicates: predicates.into(),
            projections: projections.into(),
        });
    }
    out.into()
}

/// The generic parameters called `names`, in order.
fn params_of_names(names: &[Symbol]) -> Vec<Ty> {
    (names.iter().enumerate())
        .map(|(i, &name)| Ty::param(i as u32, name))
        .collect()
}

/// Reports what is wrong with type alias `id` itself: errors in its
/// type, a cycle of aliases (once, at its first alias), parameters it
/// does not use, and traits its (unenforced) bounds name that do not
/// resolve.
fn check_alias(
    items: &ItemTree,
    id: flux_hir::TypeAliasId,
    alias: &flux_hir::TypeAlias,
    lens: Option<&ArrayLens<'_>>,
    sigs: &Signatures,
    reported: &mut Vec<Diagnostic>,
) {
    let mut push = |range, message: String| {
        reported.push(Diagnostic {
            file: alias.loc.file,
            range,
            message,
        });
    };
    for p in alias.generics.predicates.iter() {
        if let (_, Some((range, message))) = items.resolve_trait(alias.loc.module, &p.bound) {
            push(range, message);
        }
    }
    if alias_cycle(items, id) {
        let earlier = items
            .type_aliases
            .iter()
            .take(id.index())
            .any(|(other, _)| {
                reaches_alias(items, other, id, &mut Vec::new())
                    && reaches_alias(items, id, other, &mut Vec::new())
            });
        if !earlier {
            push(
                alias.ty_range,
                format!("cycle detected when expanding type alias `{}`", alias.name),
            );
        }
        return;
    }
    let mut errors = Vec::new();
    let names = generic_names(items, None, &alias.generics);
    let consts = generic_consts(items, None, &alias.generics);
    let unsized_flags = flux_solve::unsized_params(items, None, &alias.generics);
    let scope = TypeScope {
        generics: &names,
        consts: &consts,
        unsized_params: &unsized_flags,
        signature_of: Some("type aliases"),
        lens,
        defaults: Some(&sigs.trait_defaults),
        struct_fields: Some(&sigs.structs),
        ..TypeScope::default()
    };
    let bounds =
        traits::resolve_predicates(items, alias.loc.module, &alias.generics, scope, &mut errors);
    check_type_defaults(
        items,
        &alias.generics,
        alias.loc.module,
        TypeScope {
            bounds: &bounds.predicates,
            ..scope
        },
        &mut errors,
    );
    let ty = expand_alias(items, id, lens, Some(&sigs.trait_defaults), &mut errors);
    for (range, message) in errors {
        push(range, message);
    }
    if ty == Ty::ERROR {
        return;
    }
    for (i, param) in alias.generics.params.iter().enumerate() {
        if !param.is_const() && !uses_param(ty, i as u32) {
            push(
                param.range,
                format!("type parameter `{}` is never used", param.name),
            );
        }
    }
}

/// The innermost type preventing a shared static from being `Sync`.
/// Use the same rules as bounds, including explicit unsafe impls.
fn not_sync(items: &ItemTree, sigs: &Signatures, ty: Ty, seen: &mut Vec<StructId>) -> Option<Ty> {
    let ty = flux_solve::traits::normalize(items, sigs, &[], ty);
    let sync = *items.lang_traits.get(&Symbol::intern("sync"))?;
    let goal = traits::Predicate {
        self_ty: ty,
        trait_ref: traits::TraitRef {
            trait_: sync,
            name: items.traits[sync].name,
            args: Ty::list(&[]),
        },
    };
    if flux_solve::traits::holds(items, sigs, &[], goal) {
        return None;
    }
    if let TyKind::Adt(adt, _) = ty.kind() {
        if seen.contains(&adt.strukt) {
            return Some(ty);
        }
        seen.push(adt.strukt);
    }
    if let Some(nested) = flux_solve::traits::auto_trait_requirements(items, sigs, goal) {
        for p in nested {
            if let Some(cause) = not_sync(items, sigs, p.self_ty, seen) {
                return Some(cause);
            }
        }
    }
    Some(ty)
}

/// Whether a value of `ty`, which is `strukt` itself at the top, contains
/// another `strukt` by value; `seen` guards against cycles that do not
/// involve `strukt`.
fn contains(sigs: &Signatures, ty: Ty, strukt: StructId, seen: &mut Vec<StructId>) -> bool {
    if let TyKind::Adt(adt, _) = ty.kind() {
        if adt.strukt == strukt && !seen.is_empty() {
            return true;
        }
        if seen.contains(&adt.strukt) {
            return false;
        }
        seen.push(adt.strukt);
    }
    let fields = flux_solve::contents(sigs, ty).unwrap_or(&[]);
    fields
        .iter()
        .any(|&field| contains(sigs, field, strukt, seen))
}

/// The type of enum `strukt`'s discriminant: its `repr`'s, or `isize`.
pub fn discriminant_ty(items: &ItemTree, strukt: StructId) -> IntTy {
    items.structs[strukt]
        .repr
        .and_then(IntTy::from_name)
        .unwrap_or(IntTy::Isize)
}

/// The fields of a struct, or of every variant of an enum in turn: what
/// [`Signatures::structs`] has the types of.
pub fn all_fields(s: &flux_hir::Struct) -> impl Iterator<Item = &flux_hir::Field> {
    s.fields
        .iter()
        .chain(s.variants.iter().flatten().flat_map(|v| v.fields.iter()))
}

/// The type of `impl` block `imp` (`S<T>` in `impl<T> S<T>`), in terms of
/// its parameters; an error if its path did not resolve (reported).
fn impl_type(
    items: &ItemTree,
    sigs: &Signatures,
    imp: &Impl,
    lens: Option<&ArrayLens<'_>>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let generics: Vec<Symbol> = imp.generics.params.iter().map(|p| p.name).collect();
    let consts: Vec<Option<Ty>> = imp.generics.params.iter().map(param_const_ty).collect();
    let mut errors = Vec::new();
    check_const_params(items, imp.module, &imp.generics, &generics, &mut errors);
    diagnostics.extend(errors.into_iter().map(|(range, message)| Diagnostic {
        file: imp.file,
        range,
        message,
    }));
    let scope = TypeScope {
        self_ty: None,
        generics: &generics,
        consts: &consts,
        signature_of: Some("implementations"),
        lens,
        ..TypeScope::default()
    };
    // The impl header may project through its own generic bounds, as in
    // `impl<A: Array> Trait for [A::Item]`. The later signature pass checks
    // these predicates completely once trait defaults and supertraits exist.
    let preliminary = resolve_predicates(
        items,
        sigs,
        imp.module,
        &imp.generics,
        scope,
        &mut Vec::new(),
    );
    let bounds = flux_solve::traits::elaborate(sigs, &preliminary.predicates);
    let scope = TypeScope {
        bounds: &bounds,
        ..scope
    };
    let mut errors = Vec::new();
    if imp.trait_ref.is_some() {
        // Only the diagnostics added here: looking through all of them
        // for each `impl` block would be quadratic.
        let start = diagnostics.len();
        let ty = resolve_or_report(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            diagnostics,
        );
        for d in &mut diagnostics[start..] {
            d.file = imp.file;
        }
        return ty;
    }
    let through_alias = |path: &Path| {
        matches!(
            items.resolve(imp.module, path, Ns::Types),
            Ok(Resolved::Def(Def::TypeAlias(_)))
                | Err(ResolveError::Private {
                    resolved: Resolved::Def(Def::TypeAlias(_)),
                    ..
                })
        )
    };
    let ty = match (imp.target, &imp.self_ty) {
        // `impl A` for `type A = S<u8>;`: the alias's type.
        (Some(ImplTarget::Struct(_)), TypeRef::Path(path)) if through_alias(path) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Struct(strukt)), TypeRef::Path(path)) => struct_type(
            items,
            imp.module,
            strukt,
            path,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (
            Some(ImplTarget::Primitive(_)),
            TypeRef::Slice(_) | TypeRef::Array { .. } | TypeRef::Ptr { .. },
        ) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Primitive(name)), _) => primitive(name).unwrap_or(Ty::ERROR),
        (Some(ImplTarget::Object(_)), TypeRef::Dyn(_)) => resolve_type(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            &mut errors,
        ),
        _ => Ty::ERROR,
    };
    diagnostics.extend(errors.into_iter().map(|(at, message)| Diagnostic {
        file: imp.file,
        range: at,
        message,
    }));
    ty
}

/// Checks that a program's `main` exists and has the right shape.
pub fn check_main(items: &ItemTree, sigs: &Signatures) -> Option<Diagnostic> {
    let krate = items.main_crate();
    let root = items.module_of(krate);
    let data = &items.crates[krate.0 as usize];
    if data.no_std && !items.std_available(root) {
        return Some(Diagnostic {
            file: items.modules[root].file,
            range: TextRange::empty(0),
            message: "`no_std` executables without `extern crate std` are not supported yet".into(),
        });
    }
    let main = items.functions.iter().find(|(_, f)| {
        f.name.as_str() == items.entry_function_name(krate)
            && !f.is_extern
            && f.owner.is_none()
            && f.loc.module == root
    });
    let Some((id, function)) = main else {
        let name = items.crates[krate.0 as usize].name;
        let mut d = Diagnostic::new(
            TextRange::empty(0),
            format!("`main` function not found in crate `{name}`"),
        );
        d.file = items.modules[root].file;
        return Some(d);
    };
    let sig = &sigs[id];
    let message = if !function.generics.is_empty() {
        let mut d = Diagnostic::new(
            function.generics.range,
            "`main` function is not allowed to have generic parameters",
        );
        d.file = function.loc.file;
        return Some(d);
    } else if function.abi.is_c() {
        "`main` function must have the Rust ABI".to_owned()
    } else if function.target_features.is_some() {
        "`main` function is not allowed to have `target_feature`".to_owned()
    } else if function.is_unsafe {
        "`main` function is not allowed to be `unsafe`".to_owned()
    } else if !sig.params.is_empty() {
        "`main` function cannot take parameters".to_owned()
    } else if ![Ty::UNIT, Ty::NEVER, Ty::ERROR].contains(&sig.ret)
        && !entry::termination_trait(items).is_some_and(|trait_| {
            flux_solve::traits::holds(
                items,
                sigs,
                &[],
                traits::Predicate {
                    self_ty: sig.ret,
                    trait_ref: traits::TraitRef {
                        trait_,
                        name: items.traits[trait_].name,
                        args: Ty::list(&[]),
                    },
                },
            )
        })
    {
        format!("`main` has invalid return type `{}`", sig.ret)
    } else {
        return None;
    };
    let mut d = Diagnostic::new(function.name_range, message);
    d.file = function.loc.file;
    Some(d)
}

/// A file with every body inferred.
#[derive(Debug)]
pub struct TypedFile {
    pub sigs: Signatures,
    pub bodies: ArenaMap<Function, InferenceResult>,
    pub consts: ArenaMap<Const, InferenceResult>,
    /// From signatures and every body, in source order.
    pub diagnostics: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// Whether a trait path in a bound or an `impl` block did not resolve:
/// rustc stops before type checking then.
fn unresolved_trait(items: &ItemTree) -> bool {
    let unresolved =
        |module, bound: &flux_hir::Bound| items.resolve_trait(module, bound).0.is_none();
    let in_generics = |module, generics: &Generics| {
        generics
            .predicates
            .iter()
            .any(|p| unresolved(module, &p.bound))
    };
    items.impls.values().any(|imp| {
        (imp.trait_ref.is_some() && imp.trait_.is_none()) || in_generics(imp.module, &imp.generics)
    }) || items.traits.values().any(|t| {
        in_generics(t.loc.module, &t.generics)
            || t.supertraits.iter().any(|b| unresolved(t.loc.module, b))
            || (t.types.iter()).any(|ty| ty.bounds.iter().any(|b| unresolved(t.loc.module, b)))
    }) || items
        .functions
        .values()
        .any(|f| in_generics(f.loc.module, &f.generics))
}

/// Infers every body of the program, and those of `core` it may call.
/// Array lengths may name only constants whose values are integer
/// literals: evaluating others needs MIR, which
/// [`check_file_with`]'s caller provides.
pub fn check_file(hir: &Hir) -> TypedFile {
    check(hir, false, &literal_lens, None, &[])
}

/// Like [`check_file`], with every body of `core` too: for tests of
/// `core` itself.
pub fn check_file_all(hir: &Hir) -> TypedFile {
    check(hir, true, &literal_lens, None, &[])
}

/// Like [`check_file`], with `eval` evaluating the constants array
/// lengths name.
pub fn check_file_with(hir: &Hir, eval: LenEvaluator<'_>) -> TypedFile {
    check(hir, false, eval, None, &[])
}

/// Checks source bodies against loaded interfaces without inferring dependency bodies.
/// Required roots are computed once from imported MIR by the compilation driver.
pub fn check_from(
    hir: &Hir,
    loaded: &flib::LoadedTypes,
    required_functions: &[FunctionId],
    eval: LenEvaluator<'_>,
) -> TypedFile {
    check(hir, false, eval, Some(loaded), required_functions)
}

/// The values of constants whose values are integer literals: what
/// array lengths can name without constant evaluation.
fn literal_lens(
    hir: &Hir,
    _: &Signatures,
    wanted: &[ConstId],
) -> (Vec<(ConstId, Option<u128>)>, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let values = wanted
        .iter()
        .map(|&konst| {
            let value =
                hir.const_body(konst)
                    .and_then(|(body, source_map)| match &body.exprs[body.root] {
                        flux_hir::Expr::Literal(flux_hir::Literal::Int { value, .. }) => {
                            Some(*value)
                        }
                        flux_hir::Expr::Literal(flux_hir::Literal::Bool(b)) => Some(u128::from(*b)),
                        _ => {
                            let mut d = Diagnostic::new(
                                source_map.exprs.get(body.root).copied().unwrap_or_default(),
                                "evaluating this constant needs MIR",
                            );
                            d.file = hir.items.consts[konst].loc.file;
                            diagnostics.push(d);
                            None
                        }
                    });
            (konst, value)
        })
        .collect();
    (values, diagnostics)
}

/// The values of `core`'s constants that its array lengths may name:
/// those whose values are literals, which need no evaluation (`static
/// mut BUF: [u8; CAPACITY]`).
fn core_lens_values() -> LenValues {
    let core = Core::get();
    literal_values(
        (core.items.consts.iter()).filter_map(|(id, _)| core.const_body(id).map(|b| (id, b))),
    )
}

/// The values of those of `consts` whose values are integer or `bool`
/// literals.
fn literal_values<'b>(
    consts: impl Iterator<Item = (ConstId, &'b (flux_hir::Body, flux_hir::BodySourceMap))>,
) -> LenValues {
    consts
        .filter_map(|(id, (body, _))| match &body.exprs[body.root] {
            flux_hir::Expr::Literal(flux_hir::Literal::Int { value, .. }) => {
                Some((id, Some(*value)))
            }
            flux_hir::Expr::Literal(flux_hir::Literal::Bool(b)) => Some((id, Some(u128::from(*b)))),
            _ => None,
        })
        .collect()
}

/// Resolves the signatures of `items`, evaluating with `eval` the
/// constants their array lengths name, if any: then they are resolved
/// again after each round of evaluation, until no length is missing.
fn lens_signatures(hir: &Hir, all: bool, eval: LenEvaluator<'_>) -> (Signatures, Vec<Diagnostic>) {
    lens_signatures_from(hir, all, eval, None)
}

fn lens_signatures_from(
    hir: &Hir,
    all: bool,
    eval: LenEvaluator<'_>,
    loaded: Option<&flib::LoadedTypes>,
) -> (Signatures, Vec<Diagnostic>) {
    let items = &hir.items;
    // Most constants lengths name are literals, known without
    // evaluation; the others need the signatures resolved first.
    let mut known = loaded.map_or_else(LenValues::default, |loaded| {
        loaded.signatures().array_lens.clone()
    });
    known.extend(literal_values(hir.const_bodies()));
    let mut arg_tys = loaded.map_or_else(ArgTys::default, |loaded| {
        loaded.signatures().arg_tys.clone()
    });
    let resolve = |lens: &ArrayLens<'_>| match loaded {
        Some(loaded) => loaded.extend_source_signatures(items, Some(lens)),
        None => signatures_checked(items, all, Some(lens)),
    };
    let mut failures = Vec::new();
    loop {
        let lens = ArrayLens::new(&known, &arg_tys);
        let (mut sigs, mut diagnostics) = resolve(&lens);
        let missing = lens.missing.into_inner();
        let new_tys = lens.new_arg_tys.into_inner();
        if missing.is_empty() && new_tys.is_empty() {
            sigs.array_lens = known;
            sigs.arg_tys = arg_tys;
            diagnostics.append(&mut failures);
            return (sigs, diagnostics);
        }
        arg_tys.extend(new_tys);
        for (&konst, &ty) in &arg_tys {
            sigs.consts.insert(konst, ty);
        }
        sigs.arg_tys = arg_tys.clone();
        sigs.array_lens = known.clone();
        // The evaluator leaves out a length whose constant names an item
        // whose type waits for another length in `missing`
        // (`[u64; C::ALL.len()]` with `ALL: [C; [A, B].len()]`): it is
        // missing again once the signatures are resolved with that length.
        let (values, mut round) = eval(hir, &sigs, &missing);
        failures.append(&mut round);
        let before = known.len();
        known.extend(values);
        if known.len() == before {
            // No length was evaluated: what is left waits on itself.
            for konst in missing {
                known.entry(konst).or_insert(None);
            }
        }
    }
}

/// The embedded core, alloc and std destructors, which implicit drop glue
/// may call even when no explicit call reaches their bodies.
pub fn core_drop_fns(items: &ItemTree) -> Vec<FunctionId> {
    let Some(&t) = items.lang_traits.get(&sym::drop) else {
        return Vec::new();
    };
    let impls = items.trait_impls.get(&t).into_iter().flatten();
    impls
        .flat_map(|&imp| items.impls[imp].functions.iter().copied())
        // std shares the embedded cache but cannot define trusted lang items.
        // Its destructors are still roots, just like core and alloc's.
        .filter(|f| f.index() < items.core.functions as usize)
        .collect()
}

/// `core`'s functions code generation calls without MIR naming them:
/// those a failed check panics through, and what the entry point calls
/// before and after `main`, by their `#[lang]` names.
pub const RUNTIME_LANG_FNS: &[&str] = &[
    "args_init",
    "coroutine_alloc",
    "coroutine_free",
    "panic_bounds_check",
    "panic_const_div_by_zero",
    "panic_const_rem_by_zero",
    "panic_const_div_overflow",
    "panic_const_rem_overflow",
    "flush_stdout",
];

/// `core`'s functions every program may need whether or not its code
/// calls them: its `Drop` impls, which drop glue may call, and
/// [`RUNTIME_LANG_FNS`].
pub fn core_roots(items: &ItemTree) -> Vec<FunctionId> {
    let mut roots = core_drop_fns(items);
    roots.extend(
        RUNTIME_LANG_FNS
            .iter()
            .filter_map(|lang| items.lang_fns.get(&Symbol::intern(lang)).copied()),
    );
    roots
}

/// Compiler-invoked unwind runtime entry points in the trusted library prefix.
/// Type checking validates them even before a MIR strategy selects their code.
pub fn unwind_runtime_roots(items: &ItemTree) -> Vec<FunctionId> {
    items
        .functions
        .iter()
        .filter_map(|(id, function)| {
            (id.into_raw() < items.core.functions
                && !function.is_extern
                && matches!(
                    function.link_name.map(|name| name.as_str()),
                    Some(
                        "__flux_panic_str"
                            | "__flux_panic_fmt"
                            | "__flux_eh_personality"
                            | "__flux_panic_finish"
                    )
                ))
            .then_some(id)
        })
        .collect()
}

/// `Deref::deref`, or `DerefMut::deref_mut` for `Mut`: what an
/// overloaded dereference calls.
pub fn deref_fn(items: &ItemTree, mutability: Mutability) -> Option<FunctionId> {
    let (lang, name) = match mutability {
        Mutability::Not => ("deref", "deref"),
        Mutability::Mut => ("deref_mut", "deref_mut"),
    };
    let t = *items.lang_traits.get(&Symbol::intern(lang))?;
    items.traits[t].function(items, Symbol::intern(name))
}

/// The functions a call of `function` may run: itself, and for a
/// trait's method, its definition in each `impl` block of the trait.
pub fn call_targets(items: &ItemTree, function: FunctionId) -> Vec<FunctionId> {
    let mut out = vec![function];
    // Both ordinary definitions must be type/MIR checked before mono chooses
    // one concrete instance. Metadata contains already-resolved trusted IDs.
    if let Some(dispatch) = items.sized_dispatch.get(&function) {
        out.extend([dispatch.sized, dispatch.fallback]);
    }
    if let Some(Owner::Trait(t)) = items.functions[function].owner {
        if Some(t) == fn_trait(items, ClosureKind::FnOnce)
            && let Some(&helper) = items.lang_fns.get(&Symbol::intern("box_call_once"))
        {
            // The owning vtable slot is a helper, rather than the unsized
            // by-value trait method. Check its body before instance collection.
            out.push(helper);
        }
        let name = items.functions[function].name;
        let impls = items.trait_impls.get(&t).into_iter().flatten();
        out.extend(impls.flat_map(|&imp| {
            items.impls[imp]
                .functions
                .iter()
                .copied()
                .filter(move |&f| items.functions[f].name == name)
        }));
    }
    out
}

fn check(
    hir: &Hir,
    all: bool,
    eval: LenEvaluator<'_>,
    loaded: Option<&flib::LoadedTypes>,
    required_functions: &[FunctionId],
) -> TypedFile {
    let (mut sigs, mut diagnostics) = lens_signatures_from(hir, all, eval, loaded);
    let mut bodies = ArenaMap::new();
    if sigs.cyclic || unresolved_trait(&hir.items) {
        Diagnostic::sort(&mut diagnostics);
        return TypedFile {
            sigs,
            bodies,
            consts: ArenaMap::new(),
            warnings: Vec::new(),
            diagnostics,
        };
    }
    let items = &hir.items;
    let mut queue: Vec<FunctionId> = Vec::new();
    // The functions whose targets are queued already: a trait method
    // called from every body would queue all its blocks' for each.
    let mut expanded = rustc_hash::FxHashSet::default();
    let mut reached = |result: &InferenceResult, queue: &mut Vec<FunctionId>| {
        let called = result.calls.iter().map(|(_, i)| i.function);
        let methods = result.methods.iter().map(|(_, m)| m.function);
        let operators = result.overloads.iter().map(|(_, i)| i.function);
        let conversions = result.try_conversions.iter().map(|(_, i)| i.function);
        // Functions named as values (`.map(Some)`), which their
        // `Fn`-family impls call.
        let values = result
            .expr_types
            .iter()
            .filter_map(|(_, ty)| match ty.kind() {
                TyKind::FnDef(def) => Some(def.function),
                _ => None,
            });
        // A value made a trait object may have any method of its
        // trait and supertraits called, through the vtable, and be
        // dropped in place through it.
        let traits: Vec<flux_hir::TraitId> = result
            .unsizes
            .iter()
            .filter_map(|(_, &ty)| objects::unsize_trait(items, ty))
            .collect();
        let drop = (!traits.is_empty())
            .then(|| objects::drop_in_place(items))
            .flatten();
        let objects: Vec<FunctionId> = traits
            .into_iter()
            .flat_map(|t| flux_solve::objects::supertraits_of(items, t))
            .flat_map(|t| items.traits[t].functions.iter().copied())
            .chain(drop)
            .collect();
        // `Deref::deref` and `DerefMut::deref_mut`, of overloaded
        // dereferences.
        let derefs = result
            .overloaded_derefs
            .iter()
            .chain(result.coercion_derefs.iter())
            .flat_map(|(_, steps)| steps.iter())
            .filter_map(|step| deref_fn(items, step.mutability));
        for f in called
            .chain(methods)
            .chain(operators)
            .chain(conversions)
            .chain(values)
            .chain(objects)
            .chain(derefs)
        {
            if expanded.insert(f) {
                queue.extend(call_targets(items, f));
            }
        }
    };
    // What this compilation does not infer: the embedded libraries'
    // bodies, inferred once per process, unless every body is checked.
    let library = (!all).then(|| LibraryTypes::of(hir)).flatten();
    let origin = |owner| flib::body_origin(library, loaded, owner);
    let mut consts = ArenaMap::new();
    for (id, (body, source_map)) in hir.const_bodies() {
        let result = match origin(ClosureOwner::Const(id)) {
            flib::BodyOrigin::Library => library
                .expect("library ownership has a provider")
                .konst(id)
                .clone(),
            flib::BodyOrigin::Imported => continue,
            flib::BodyOrigin::Source => infer_const(items, &sigs, id, body, source_map),
        };
        reached(&result, &mut queue);
        consts.insert(id, result);
    }
    // `core`'s `Drop` impls, which drop glue may call, and the
    // functions code generation calls, then the program's bodies, then
    // the `core` bodies they reach.
    queue.extend(core_roots(items));
    queue.extend(unwind_runtime_roots(items));
    queue.extend_from_slice(required_functions);
    queue.extend(entry::main_runner(items, &sigs));
    queue.extend(
        hir.bodies
            .iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>()
            .into_iter()
            .rev(),
    );
    if all {
        queue.extend(items.functions.iter().map(|(id, _)| id));
    }
    let mut pending_opaque_dependencies = rustc_hash::FxHashSet::default();
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() {
            continue;
        }
        let result = match origin(ClosureOwner::Function(id)) {
            flib::BodyOrigin::Library => {
                match library.expect("library ownership has a provider").body(id) {
                    Some(result) => result.clone(),
                    None => continue,
                }
            }
            flib::BodyOrigin::Imported => continue,
            flib::BodyOrigin::Source => {
                let Some((body, source_map)) = hir.body(id) else {
                    continue;
                };
                infer_body(items, &sigs, id, body, source_map)
            }
        };
        let providers: Vec<_> = result
            .calls
            .iter()
            .map(|(_, call)| call.function)
            .filter(|&provider| {
                provider != id
                    && bodies.get(provider).is_none()
                    && !pending_opaque_dependencies.contains(&provider)
                    && sigs
                        .opaques
                        .get(provider)
                        .is_some_and(|types| !types.is_empty())
                    && hir.body(provider).is_some()
            })
            .collect();
        if !providers.is_empty() && pending_opaque_dependencies.insert(id) {
            queue.push(id);
            queue.extend(providers);
            continue;
        }
        for (index, &hidden) in result.opaques.iter().enumerate() {
            sigs.hidden.insert((id, index as u32), hidden);
        }
        pending_opaque_dependencies.remove(&id);
        reached(&result, &mut queue);
        bodies.insert(id, result);
    }
    // Array lengths in bodies that name constants not evaluated yet:
    // evaluated now, and those bodies inferred again; so are the
    // constants of const generic arguments whose types they found.
    let mut missing: Vec<ConstId> = Vec::new();
    let mut new_tys: Vec<ConstId> = Vec::new();
    let results = consts
        .iter()
        .map(|(_, r)| r)
        .chain(bodies.iter().map(|(_, r)| r));
    for result in results {
        for &c in &result.missing_lens {
            if !missing.contains(&c) {
                missing.push(c);
            }
        }
        for &(c, ty) in &result.new_arg_tys {
            if sigs.arg_tys.insert(c, ty).is_none() {
                sigs.consts.insert(c, ty);
                new_tys.push(c);
            }
        }
    }
    if !missing.is_empty() || !new_tys.is_empty() {
        let (values, failures) = eval(hir, &sigs, &missing);
        sigs.array_lens.extend(values);
        // Signatures are not resolved again here, so a length the
        // evaluator left out for that fails.
        for &konst in &missing {
            sigs.array_lens.entry(konst).or_insert(None);
        }
        diagnostics.extend(failures);
        let redo: Vec<ConstId> = consts
            .iter()
            .filter(|(id, r)| !r.missing_lens.is_empty() || new_tys.contains(id))
            .map(|(id, _)| id)
            .collect();
        for id in redo {
            let (body, source_map) = hir.const_body(id).expect("inferred above");
            consts.insert(id, infer_const(items, &sigs, id, body, source_map));
        }
        let redo: Vec<FunctionId> = bodies
            .iter()
            .filter(|(_, r)| !r.missing_lens.is_empty() || !r.new_arg_tys.is_empty())
            .map(|(id, _)| id)
            .collect();
        for id in redo {
            let (body, source_map) = hir.body(id).expect("inferred above");
            bodies.insert(id, infer_body(items, &sigs, id, body, source_map));
        }
    }
    // What code generation sees of opaque types.
    for (id, result) in bodies.iter() {
        for (i, &ty) in result.opaques.iter().enumerate() {
            sigs.hidden.insert((id, i as u32), ty);
        }
    }
    fn recursive_opaque(sigs: &Signatures, ty: Ty, target: (FunctionId, u32)) -> bool {
        let mut pending = vec![ty];
        let mut seen = rustc_hash::FxHashSet::default();
        while let Some(ty) = pending.pop() {
            if !ty.has_opaques() {
                continue;
            }
            let mut recursive = false;
            ty.map_opaques(&mut |opaque| {
                let key = (opaque.function, opaque.index);
                if key == target {
                    recursive = true;
                } else if seen.insert(key)
                    && let Some(&hidden) = sigs.hidden.get(&key)
                {
                    pending.push(hidden);
                }
                Ty::opaque(opaque)
            });
            if recursive {
                return true;
            }
        }
        false
    }
    for (id, result) in bodies.iter() {
        for (index, &hidden) in result.opaques.iter().enumerate() {
            if recursive_opaque(&sigs, hidden, (id, index as u32)) {
                let mut diagnostic = Diagnostic::new(
                    items.functions[id].def_range,
                    "cannot resolve recursive opaque type",
                );
                diagnostic.file = items.functions[id].loc.file;
                diagnostics.push(diagnostic);
            }
        }
    }
    for (id, result) in consts.iter() {
        let file = items.consts[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for (id, result) in bodies.iter() {
        let file = items.functions[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for diagnostic in &mut diagnostics {
        items.resolve_diagnostic(diagnostic);
    }
    Diagnostic::sort(&mut diagnostics);
    let (expectation_errors, mut warnings) = items.overflow_expectation_diagnostics();
    diagnostics.extend(expectation_errors.into_iter().map(|mut d| {
        items.resolve_diagnostic(&mut d);
        d
    }));
    Diagnostic::sort(&mut diagnostics);
    warnings.extend(items.lint_warnings());
    for (id, result) in bodies.iter() {
        let file = items.functions[id].loc.file;
        warnings.extend(
            result
                .warnings
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for (id, result) in consts.iter() {
        let file = items.consts[id].loc.file;
        warnings.extend(
            result
                .warnings
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
    }
    for diagnostic in &mut warnings {
        items.resolve_diagnostic(diagnostic);
    }
    Diagnostic::sort(&mut warnings);
    warnings.dedup_by(|a, b| a.file == b.file && a.range == b.range && a.message == b.message);
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics,
        warnings,
    }
}

/// An evaluated constant used by pattern checking. Structural aggregates
/// retain their field values; shared strings remain refutable without their
/// contents because finite string patterns cannot exhaust the domain.
#[derive(Debug, Clone)]
pub enum PatternConst {
    /// An evaluated shared string. Its contents are compared in MIR; a finite
    /// set of strings cannot exhaust the string domain.
    Str,
    /// Exact elements of a shared byte-slice constant pattern.
    Bytes(Vec<u8>),
    Scalar(i128),
    Aggregate(Vec<PatternConst>),
    Variant {
        index: u32,
        fields: Vec<PatternConst>,
    },
}

/// Whether a constant of `ty` can be matched structurally. ADTs require
/// derived PartialEq; a handwritten equality method cannot define patterns.
/// Enum payloads are checked against the evaluated active variant; references
/// and pointer constants remain outside this subset.
pub fn structural_pattern_type(items: &ItemTree, sigs: &Signatures, ty: Ty) -> bool {
    fn check(items: &ItemTree, sigs: &Signatures, ty: Ty, depth: u32) -> bool {
        if depth >= 128 {
            return false;
        }
        match ty.kind() {
            TyKind::Int(_) | TyKind::Bool | TyKind::Char | TyKind::Error => true,
            TyKind::Tuple(fields) => fields.iter().all(|&t| check(items, sigs, t, depth + 1)),
            TyKind::Array(elem, len) => {
                len.as_usize().is_some() && check(items, sigs, elem, depth + 1)
            }
            TyKind::Adt(adt, _) => {
                !items.structs[adt.strukt].is_union
                    && items
                        .functions
                        .values()
                        .any(|f| f.derived == Some((adt.strukt, flux_hir::Derive::PartialEq)))
                    && (sigs.is_enum(adt.strukt)
                        || flux_solve::fields(sigs, ty).is_some_and(|fields| {
                            fields.iter().all(|&t| check(items, sigs, t, depth + 1))
                        }))
            }
            _ => false,
        }
    }
    check(items, sigs, ty, 0)
}

/// The checks of the patterns of the bodies whose patterns name
/// constants ([`InferenceResult::deferred_pats`]), given `value`, the
/// scalar leaves and aggregate fields of a structural constant, or
/// `None` if it has no value (reported). Bodies that name one without a
/// value are not checked. Also returns the functions with errors, which
/// rustc does not borrow-check.
pub fn check_deferred_patterns(
    hir: &Hir,
    typed: &TypedFile,
    value: &mut dyn FnMut(ConstInstance) -> Option<PatternConst>,
) -> (Vec<Diagnostic>, Vec<FunctionId>) {
    let functions = typed.bodies.iter().map(|(id, result)| {
        let file = hir.items.functions[id].loc.file;
        (
            Some(id),
            hir.body(id),
            file,
            hir.items.functions[id].loc.module,
            result,
        )
    });
    let consts = typed.consts.iter().map(|(id, result)| {
        let file = hir.items.consts[id].loc.file;
        (
            None,
            hir.const_body(id),
            file,
            hir.items.consts[id].loc.module,
            result,
        )
    });
    let mut diagnostics = Vec::new();
    let mut failed = Vec::new();
    'bodies: for (function, body, file, module, result) in functions.chain(consts) {
        let Some((body, source_map)) = body else {
            continue;
        };
        if !result.deferred_pats {
            continue;
        }
        let mut values = ArenaMap::new();
        for data in body.pats.values() {
            let ends = match *data {
                flux_hir::Pat::Expr(expr) => [Some(expr), None],
                flux_hir::Pat::Range { start, end, .. } => [start, end],
                _ => continue,
            };
            for expr in ends.into_iter().flatten() {
                let instance = match (result.consts.get(expr), &body.exprs[expr]) {
                    (Some(&instance), _) => instance,
                    (None, &flux_hir::Expr::Path(flux_hir::Res::Const(konst))) => {
                        ConstInstance { konst, args: &[] }
                    }
                    _ => continue,
                };
                match value(instance) {
                    Some(v) => {
                        values.insert(expr, v);
                    }
                    None => continue 'bodies,
                }
            }
        }
        let found = pats::check_body(
            &hir.items,
            module,
            &typed.sigs,
            body,
            source_map,
            &result.pat_types,
            &result.pat_derefs,
            &result.expr_types,
            &values,
            &[],
        );
        if !found.is_empty() {
            failed.extend(function);
        }
        diagnostics.extend(found.into_iter().map(|d| Diagnostic { file, ..d }));
    }
    (diagnostics, failed)
}

impl TypedFile {
    /// Each function's signature and the type of each of its bindings,
    /// then the diagnostics. Used by snapshot tests.
    pub fn debug_dump(&self, hir: &Hir) -> String {
        let mut out = String::new();
        let items = &hir.items;
        for (id, s) in items.structs.iter() {
            if !items.in_main_crate(s.loc.module) {
                continue;
            }
            if let Some(variants) = &s.variants {
                let starts = &self.sigs.variant_starts[id];
                let all = self.sigs.structs[id];
                let shown: Vec<String> = variants
                    .iter()
                    .enumerate()
                    .map(|(i, v)| {
                        let types = &all[starts[i] as usize..starts[i + 1] as usize];
                        let fields: Vec<String> = v
                            .fields
                            .iter()
                            .zip(types)
                            .map(|(f, ty)| format!("{}: {ty}", f.name))
                            .collect();
                        match fields.as_slice() {
                            [] => v.name.to_string(),
                            _ => format!("{} {{ {} }}", v.name, fields.join(", ")),
                        }
                    })
                    .collect();
                writeln!(
                    out,
                    "enum {}{} {{ {} }}",
                    s.name,
                    s.generics,
                    shown.join(", ")
                )
                .unwrap();
                continue;
            }
            let fields: Vec<String> = s
                .fields
                .iter()
                .zip(self.sigs.structs[id])
                .map(|(f, ty)| format!("{}: {ty}", f.name))
                .collect();
            match fields.as_slice() {
                [] => writeln!(out, "struct {}{} {{}}", s.name, s.generics),
                _ => writeln!(
                    out,
                    "struct {}{} {{ {} }}",
                    s.name,
                    s.generics,
                    fields.join(", ")
                ),
            }
            .unwrap();
        }
        for (id, function) in items.functions.iter() {
            if !hir.is_program(id) || function.ctor.is_some() {
                continue;
            }
            let sig = &self.sigs[id];
            let params: Vec<String> = sig.params.iter().map(Ty::to_string).collect();
            writeln!(
                out,
                "fn {}{}({}) -> {}",
                function.name,
                function.generics,
                params.join(", "),
                sig.ret
            )
            .unwrap();
            if let (Some((body, _)), Some(result)) = (hir.bodies.get(id), self.bodies.get(id)) {
                for (binding, data) in body.bindings.iter() {
                    let ty = result
                        .binding_types
                        .get(binding)
                        .copied()
                        .unwrap_or(Ty::ERROR);
                    writeln!(out, "    {}@{}: {ty}", data.name, binding.index()).unwrap();
                }
            }
        }
        let lens = ArrayLens::new(&self.sigs.array_lens, &self.sigs.arg_tys);
        for (id, alias) in items.type_aliases.iter() {
            if items.in_main_crate(alias.loc.module) {
                let ty = expand_alias(
                    items,
                    id,
                    Some(&lens),
                    Some(&self.sigs.trait_defaults),
                    &mut Vec::new(),
                );
                writeln!(out, "type {}{} = {ty}", alias.name, alias.generics).unwrap();
            }
        }
        for (id, konst) in items.consts.iter() {
            if items.in_main_crate(konst.loc.module) {
                let name = items.const_name(id);
                writeln!(out, "const {name}: {}", self.sigs.consts[id]).unwrap();
            }
        }
        hir.write_diagnostics(&mut out, &self.diagnostics);
        out
    }
}

/// `f` as written in messages.
fn function_name(items: &ItemTree, f: FunctionId) -> Symbol {
    items.functions[f].name
}

/// A sized subset with generic parameters. Check every substituted field,
/// including enum variants and nested unions, rather than treating an ADT's
/// Copy impl as proof that its representation is supported. Generic arguments
/// retain their region slots; direct stored-reference declarations remain gated.
/// Reject recursive
/// generic arguments conservatively, even when they occur only in phantom data.
fn supported_union_field(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    checked: &mut FxHashMap<Ty, bool>,
    depth: u32,
) -> bool {
    let ty = flux_solve::traits::normalize(items, sigs, &[], ty);
    if ty.has_infer() || depth >= 128 {
        return false;
    }
    if let Some(&supported) = checked.get(&ty) {
        return supported;
    }
    // A repeated in-progress type is a cycle. Memoize finished types too,
    // so repeated fields do not expand the same representation exponentially.
    // The depth bound also stops expanding generic cycles such as S<S<T>>.
    checked.insert(ty, false);
    let supported = match ty.kind() {
        TyKind::Param(_) => true,
        TyKind::Int(_) | TyKind::Float(_) | TyKind::Bool | TyKind::Char => true,
        TyKind::Tuple(fields) => fields
            .iter()
            .all(|&field| supported_union_field(items, sigs, field, checked, depth + 1)),
        // A raw pointer stores only its address. Pointee fields may be
        // recursive and need no expansion; metadata and region slots still
        // have to fit the region-free representation of this union subset.
        TyKind::RawPtr(_, pointee) => regions::slots(items, ty) == 0 && !sigs.is_unsized(pointee),
        // Function pointers carry no borrowed value. Their parameter references
        // are late-bound; signature resolution rejects named lifetimes and
        // reference returns that would require tracking regions in the pointer.
        // Do not recurse into the signature as though its inputs were storage.
        TyKind::FnPtr(_) => true,
        TyKind::Array(elem, len) => {
            (len.as_usize().is_some() || len.has_params())
                && supported_union_field(items, sigs, elem, checked, depth + 1)
        }
        TyKind::Adt(adt, args) => {
            // Region slots also occur in type arguments, even when they are
            // phantom. Check these with the same memoized traversal rather
            // than expanding a shared generic type's region slots repeatedly.
            items.structs[adt.strukt].generics.lifetimes.is_empty()
                && args.iter().all(|&arg| {
                    matches!(arg.kind(), TyKind::Value(_))
                        || supported_union_field(items, sigs, arg, checked, depth + 1)
                })
                && flux_solve::contents(sigs, ty).is_some_and(|fields| {
                    fields
                        .iter()
                        .all(|&field| supported_union_field(items, sigs, field, checked, depth + 1))
                })
        }
        _ => false,
    };
    checked.insert(ty, supported);
    supported
}
