//! Resolve written types and declaration-scoped generic arguments.
use super::{
    ArgTys, ConstId, Def, Diagnostic, FloatTy, FnSigTy, FunctionId, Generics, Hir, ImplTarget,
    IntTy, ItemTree, LayeredArenaMap, LenValues, ModuleId, Mutability, Ns, OpaqueTy, Owner, Path,
    Predicate, ResolveError, Resolved, Signatures, Struct, StructId, Symbol, TextRange, TraitRef,
    Ty, TyKind, TypeRef, fn_regions, kw, metadata, objects, params_of_names, regions, sym, traits,
    ty,
};

/// What the names in a type mean besides items: `Self`, if anything,
/// and the generic parameters in scope, by index.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScope<'a> {
    pub self_ty: Option<Ty>,
    pub generics: &'a [Symbol],
    /// The type of each of `generics` that is a const parameter, `None`
    /// for a type parameter ([`generic_consts`]); empty where they are
    /// not known, and then each is taken for what its use needs.
    pub consts: &'a [Option<Ty>],
    /// In an item's signature, where `_` is not allowed, what rustc
    /// calls such items ("functions"); `None` in a body, where `_` is a
    /// type to infer.
    pub signature_of: Option<&'static str>,
    /// Function-pointer binders do not yet retain inherited projection regions.
    pub in_fn_pointer: bool,
    pub in_trait_bound: bool,
    /// The bounds in scope, with what they imply: where `T::Out` finds
    /// its trait.
    pub bounds: &'a [Predicate],
    /// In a trait's `impl` block, its trait: `Self::Out` is its type.
    pub self_trait: Option<TraitRef>,
    /// Resolved supertrait edges for inherited `Self::Out` in an impl.
    /// Keeping these separate from `bounds` avoids treating the impl being
    /// checked as an assumption when proving its declaration obligations.
    pub self_supertraits: &'a [Predicate],
    pub self_inherited_types: &'a [(TraitRef, Symbol, Ty)],
    /// In a trait's `impl` block's items, its associated types: what
    /// `Self::Out` and `<Self as Tr>::Out` are, as rustc normalizes them
    /// there (the block is the one for its `Self`).
    pub self_types: &'a [(Symbol, Ty)],
    /// The defaults of traits' parameters, for bounds that leave them
    /// out.
    pub defaults: Option<&'a traits::TraitDefaults>,
    /// The values of the constants array lengths name, where known.
    pub lens: Option<&'a ArrayLens<'a>>,
    /// In a function's signature, the function, whose opaque types its
    /// return type names.
    pub function: Option<FunctionId>,
    /// Which of `generics` have no implicit `Sized` bound
    /// ([`flux_solve::unsized_params`]); empty where all do.
    pub unsized_params: &'a [bool],
    /// Resolved fields for structural DST tails, when signatures are available.
    pub struct_fields: Option<&'a LayeredArenaMap<Struct, &'static [Ty]>>,
}

/// The values of the constants that array lengths name (`[u8; N]`), as
/// far as they are evaluated: `None` for one whose evaluation failed,
/// which is reported where it is evaluated. Type checking needs them,
/// and evaluating them needs MIR, so [`crate::check_file_with`] gets them
/// from a function that builds it.
#[derive(Debug)]
pub struct ArrayLens<'a> {
    pub values: &'a LenValues,
    /// The constants named whose values are not in `values`, as types
    /// were resolved: what to evaluate.
    pub missing: std::cell::RefCell<Vec<ConstId>>,
    /// The types of the anonymous constants of const generic arguments
    /// (`f::<{ N + 1 }>`), their parameters', as far as known.
    pub arg_tys: &'a ArgTys,
    /// Those found as types were resolved that `arg_tys` lacks.
    pub new_arg_tys: std::cell::RefCell<Vec<(ConstId, Ty)>>,
}

/// Evaluates the constants array lengths name, given the signatures
/// so far (whose [`Signatures::array_lens`] it may extend as it needs):
/// each constant's value, `None` if its evaluation failed, with the
/// diagnostics that say why.
pub type LenEvaluator<'e> =
    &'e dyn Fn(&Hir, &Signatures, &[ConstId]) -> (Vec<(ConstId, Option<u128>)>, Vec<Diagnostic>);

impl<'a> ArrayLens<'a> {
    pub fn new(values: &'a LenValues, arg_tys: &'a ArgTys) -> ArrayLens<'a> {
        ArrayLens {
            values,
            missing: Default::default(),
            arg_tys,
            new_arg_tys: Default::default(),
        }
    }

    /// The type of `konst`, the anonymous constant of a const generic
    /// argument, if known.
    pub fn arg_ty(&self, konst: ConstId) -> Option<Ty> {
        self.arg_tys.get(&konst).copied()
    }

    /// Notes that `konst`, the anonymous constant of a const generic
    /// argument, has type `ty`.
    fn found_arg_ty(&self, konst: ConstId, ty: Ty) {
        if ty != Ty::ERROR && !self.arg_tys.contains_key(&konst) {
            let mut new = self.new_arg_tys.borrow_mut();
            if !new.iter().any(|&(k, _)| k == konst) {
                new.push((konst, ty));
            }
        }
    }

    /// The value of constant `konst`, if known; one not evaluated yet is
    /// noted as missing.
    fn value(&self, konst: ConstId) -> Option<u128> {
        match self.values.get(&konst) {
            Some(value) => *value,
            None => {
                let mut missing = self.missing.borrow_mut();
                if !missing.contains(&konst) {
                    missing.push(konst);
                }
                None
            }
        }
    }
}

impl TypeScope<'_> {
    /// Whether values of `ty` may have no size known at compile time
    /// here ([`flux_solve::maybe_unsized`]).
    pub fn is_unsized(&self, items: &ItemTree, ty: Ty) -> bool {
        flux_solve::maybe_unsized_with(
            items,
            self.struct_fields,
            ty,
            self.unsized_params,
            self.bounds,
        )
    }

    /// The generic parameter called `name`, if one is in scope.
    pub fn param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        Some(Ty::param(index as u32, name))
    }

    /// The type of the const parameter called `name`, if it is one in
    /// scope (an error if that is not known).
    pub fn const_param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        match self.consts.get(index) {
            Some(ty) => *ty,
            None => Some(Ty::ERROR),
        }
    }

    /// Whether `name` is a type parameter in scope, known not to be a
    /// const one.
    fn type_param(&self, name: Symbol) -> bool {
        self.generics
            .iter()
            .rposition(|&n| n == name)
            .is_some_and(|i| matches!(self.consts.get(i), Some(None)))
    }
}

/// The types of the const parameters of an item in `owner` with its own
/// `generics`, `None` for its type parameters, in the order of
/// [`generic_names`].
pub fn generic_consts(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<Option<Ty>> {
    let mut consts = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        consts.push(None);
    }
    consts.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(param_const_ty)),
    );
    consts
}

/// The type of a const parameter, `None` for a type parameter.
pub fn param_const_ty(param: &flux_hir::GenericParam) -> Option<Ty> {
    match &param.kind {
        flux_hir::ParamKind::Type => None,
        flux_hir::ParamKind::Const { ty, .. } => Some(const_param_ty(ty)),
    }
}

/// The type of a const parameter written as `ty`: an integer type,
/// `bool` or `char` ([`check_const_params`] reports others), or an
/// error.
pub fn const_param_ty(ty: &TypeRef) -> Ty {
    let TypeRef::Path(path) = ty else {
        return Ty::ERROR;
    };
    path.as_name()
        .and_then(|name| primitive(name).ok())
        .filter(|t| t.as_int().is_some() || *t == Ty::BOOL || *t == Ty::CHAR)
        .unwrap_or(Ty::ERROR)
}

/// rustc's errors for the types of the const parameters of `generics`,
/// in `module` with the generic parameters `names` in scope: one that
/// names a generic parameter (E0770), and one that is not an integer
/// type, `bool` or `char`.
pub fn check_const_params(
    items: &ItemTree,
    module: ModuleId,
    generics: &Generics,
    names: &[Symbol],
    errors: &mut Vec<(TextRange, String)>,
) {
    for param in generics.params.iter() {
        let flux_hir::ParamKind::Const { ty, ty_range } = &param.kind else {
            continue;
        };
        let mut generic = false;
        visit_paths(ty, &mut |path| {
            if let Some(&first) = path.segments.first()
                && path.anchor == flux_hir::PathAnchor::Plain
                && (names.contains(&first) || first == kw::SelfUpper)
            {
                generic = true;
            }
        });
        if generic {
            errors.push((
                *ty_range,
                "the type of const parameters must not depend on other generic parameters"
                    .to_owned(),
            ));
            continue;
        }
        if const_param_ty(ty) != Ty::ERROR {
            continue;
        }
        // The other primitive types, which flux lacks or which are not
        // sized.
        if let TypeRef::Path(path) = ty
            && let Some(name) = path.as_name()
            && matches!(name.as_str(), "f16" | "f32" | "f64" | "f128" | "str")
        {
            errors.push((
                *ty_range,
                format!("`{name}` is forbidden as the type of a const generic parameter"),
            ));
            continue;
        }
        let resolved = resolve_type(
            items,
            module,
            ty,
            TypeScope::default(),
            *ty_range,
            &mut Vec::new(),
        );
        if resolved != Ty::ERROR
            && resolved.as_int().is_none()
            && resolved != Ty::BOOL
            && resolved != Ty::CHAR
        {
            errors.push((
                *ty_range,
                format!("`{ty}` is forbidden as the type of a const generic parameter"),
            ));
        } else if resolved != Ty::ERROR {
            errors.push((
                *ty_range,
                "const parameters of types named through aliases are not supported yet".to_owned(),
            ));
        } else {
            // Reported where the type is resolved.
            resolve_type(items, module, ty, TypeScope::default(), *ty_range, errors);
        }
    }
}

/// The generic argument `arg`, written at `range`, for `param`: a type
/// for a type parameter, a constant ([`TyKind::Value`] or a const
/// parameter) for a const one. A path is a type if it names one, and
/// otherwise a constant, as rustc reads it.
#[allow(clippy::too_many_arguments)]
pub fn resolve_generic_arg(
    items: &ItemTree,
    module: ModuleId,
    param: Option<&flux_hir::GenericParam>,
    arg: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let want = match param.map(|p| &p.kind) {
        Some(flux_hir::ParamKind::Const { ty, .. }) => const_param_ty(ty),
        _ => {
            if let TypeRef::Const(_) = arg {
                errors.push((
                    range,
                    "constant provided when a type was expected".to_owned(),
                ));
                return Ty::ERROR;
            }
            return resolve_type(items, module, arg, scope, range, errors);
        }
    };
    if names_nothing(items, module, arg, scope) {
        resolve_type(items, module, arg, scope, range, errors);
        errors.push((
            range,
            "unresolved item provided when a constant was expected".to_owned(),
        ));
        return Ty::ERROR;
    }
    let value = match arg {
        TypeRef::Const(value) => value.clone(),
        TypeRef::Infer if scope.signature_of.is_none() => {
            return Ty::infer(ty::InferVar::PLACEHOLDER);
        }
        TypeRef::Path(path) if names_type(items, module, path, scope) != Some(true) => {
            flux_hir::ArrayLen {
                kind: flux_hir::ArrayLenKind::Path(path.clone()),
                range,
            }
        }
        _ => {
            errors.push((
                range,
                "type provided when a constant was expected".to_owned(),
            ));
            return Ty::ERROR;
        }
    };
    const_value(items, module, &value, want, scope, errors).unwrap_or(Ty::ERROR)
}

/// Whether `path`, a generic argument, names a type rather than a
/// constant; `None` for a generic parameter of a kind not known.
fn names_type(
    items: &ItemTree,
    module: ModuleId,
    path: &Path,
    scope: TypeScope<'_>,
) -> Option<bool> {
    if path.qself.is_some() || path.args.iter().any(Option::is_some) {
        return Some(true);
    }
    if let Some(name) = path.as_name() {
        if let Some(index) = scope.generics.iter().rposition(|&n| n == name) {
            return scope.consts.get(index).map(Option::is_none);
        }
        if name == kw::SelfUpper {
            return Some(true);
        }
    }
    if path.segments.len() > 1
        && path.anchor == flux_hir::PathAnchor::Plain
        && (path.segments[0] == kw::SelfUpper || scope.generics.contains(&path.segments[0]))
    {
        // `Self::N`, `T::Out`: an associated constant or type, as the
        // parameter wants.
        return None;
    }
    // rustc reads a path that names nothing as a type, and reports it
    // unresolved there.
    Some(
        items.resolve(module, path, Ns::Types).is_ok()
            || items.resolve(module, path, Ns::Values).is_err(),
    )
}

/// Whether the path `arg`, a generic argument, names neither a type nor
/// a value.
fn names_nothing(items: &ItemTree, module: ModuleId, arg: &TypeRef, scope: TypeScope<'_>) -> bool {
    let TypeRef::Path(path) = arg else {
        return false;
    };
    let first = path.segments.first().copied();
    let local = path.anchor == flux_hir::PathAnchor::Plain
        && first.is_some_and(|n| n == kw::SelfUpper || scope.generics.contains(&n));
    path.qself.is_none()
        && !local
        && items.resolve(module, path, Ns::Types).is_err()
        && items.resolve(module, path, Ns::Values).is_err()
}

/// The generic arguments `written` (with their ranges) for parameters
/// `params`, as [`resolve_generic_arg`] resolves each.
pub fn resolve_generic_args(
    items: &ItemTree,
    module: ModuleId,
    params: &[flux_hir::GenericParam],
    written: &flux_hir::GenericArgs,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Vec<Ty> {
    // rustc reports only the first argument of the wrong kind (E0747).
    let wrong_kind = written
        .types
        .iter()
        .zip(&written.ranges)
        .zip(params)
        .find_map(|((arg, &range), param)| {
            let is_const = match arg {
                TypeRef::Const(_) => true,
                TypeRef::Path(path) => !names_type(items, module, path, scope)?,
                TypeRef::Infer => return None,
                _ => false,
            };
            match (param.is_const(), is_const) {
                (false, true) => Some((range, "constant provided when a type was expected")),
                (true, false) if names_nothing(items, module, arg, scope) => Some((
                    range,
                    "unresolved item provided when a constant was expected",
                )),
                (true, false) => Some((range, "type provided when a constant was expected")),
                _ => None,
            }
        });
    if let Some((range, message)) = wrong_kind {
        // rustc reports a path that names nothing before the kind mismatch.
        for (arg, &range) in written.types.iter().zip(&written.ranges) {
            if names_nothing(items, module, arg, scope) {
                resolve_type(items, module, arg, scope, range, errors);
            }
        }
        errors.push((range, message.to_owned()));
        return vec![Ty::ERROR; written.types.len()];
    }
    written
        .types
        .iter()
        .zip(&written.ranges)
        .enumerate()
        .map(|(i, (arg, &range))| {
            resolve_generic_arg(items, module, params.get(i), arg, scope, range, errors)
        })
        .collect()
}

/// The names of the generic parameters of an item in `owner` with its
/// own `generics`: an `impl` block's, or a trait's `Self` and its own,
/// then the item's.
pub fn generic_names(items: &ItemTree, owner: Option<Owner>, own: &Generics) -> Vec<Symbol> {
    let mut names = Vec::new();
    if let Some(Owner::Trait(_)) = owner {
        names.push(kw::SelfUpper);
    }
    names.extend(
        items
            .owner_generics(owner)
            .into_iter()
            .chain([own])
            .flat_map(|g| g.params.iter().map(|p| p.name)),
    );
    names
}

/// "1 generic argument", "2 generic arguments".
fn generic_arguments(n: usize) -> String {
    format!("{n} generic argument{}", if n == 1 { "" } else { "s" })
}

/// rustc's error for `got` generic arguments where a `kind` (struct,
/// function, ...) takes `want`.
pub fn wrong_arg_count(kind: &str, want: usize, got: usize) -> String {
    let verb = if got == 1 { "was" } else { "were" };
    format!(
        "{kind} takes {} but {} {verb} supplied",
        generic_arguments(want),
        generic_arguments(got)
    )
}

/// The permitted arity of type parameters, including a trailing defaulted suffix.
pub(crate) fn type_arg_count(
    kind: &str,
    params: &[flux_hir::GenericParam],
    got: usize,
) -> Option<String> {
    let want = params.len();
    let required = params.iter().take_while(|p| p.default.is_none()).count();
    if got >= required && got <= want {
        return None;
    }
    if required == want {
        return Some(wrong_arg_count(kind, want, got));
    }
    let (bound, count) = if got > want {
        ("at most", want)
    } else {
        ("at least", required)
    };
    let verb = if got == 1 { "was" } else { "were" };
    Some(format!(
        "{kind} takes {bound} {} but {} {verb} supplied",
        generic_arguments(count),
        generic_arguments(got)
    ))
}

thread_local! {
    /// Defaults can name another defaulted nominal type or alias. Detect
    /// cycles before recursively expanding the omitted argument lists.
    static DEFAULTING: std::cell::RefCell<Vec<Def>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Fill omitted arguments in declaration scope, substituting earlier arguments
/// as each dependent default is appended. Expressions call this only when a
/// nonempty argument list was written; bare paths keep their inference vars.
pub(crate) fn fill_type_defaults(
    items: &ItemTree,
    owner: Def,
    args: &mut Vec<Ty>,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) {
    let (generics, loc) = match owner {
        Def::Struct(id) => (&items.structs[id].generics, items.structs[id].loc),
        Def::TypeAlias(id) => (&items.type_aliases[id].generics, items.type_aliases[id].loc),
        _ => unreachable!("defaults belong to a nominal type or alias"),
    };
    if args.len() >= generics.len() {
        return;
    }
    let cycle = DEFAULTING.with(|active| {
        let mut active = active.borrow_mut();
        if active.contains(&owner) || active.len() >= 128 {
            return true;
        }
        active.push(owner);
        false
    });
    if cycle {
        let range = generics.params[args.len()]
            .default
            .as_ref()
            .map_or(generics.range, |d| d.1);
        errors.push((
            range,
            "cycle detected when resolving type parameter defaults".into(),
        ));
        args.resize(generics.len(), Ty::ERROR);
        return;
    }
    let names = generic_names(items, None, generics);
    let consts = generic_consts(items, None, generics);
    let unsized_flags = flux_solve::unsized_params(items, None, generics);
    let own = TypeScope {
        generics: &names,
        consts: &consts,
        unsized_params: &unsized_flags,
        signature_of: Some("generic parameter defaults"),
        lens: scope.lens,
        defaults: scope.defaults,
        struct_fields: scope.struct_fields,
        ..TypeScope::default()
    };
    while args.len() < generics.len() {
        let ty = match &generics.params[args.len()].default {
            Some((ty, range)) => resolve_generic_arg(
                items,
                loc.module,
                Some(&generics.params[args.len()]),
                ty,
                own,
                *range,
                errors,
            )
            .subst(args),
            None => Ty::ERROR,
        };
        args.push(ty);
    }
    DEFAULTING.with(|active| {
        active.borrow_mut().pop();
    });
}

/// Validate defaults even when no use omits them. References to later
/// parameters must not fall through to module items with the same names.
pub(super) fn check_type_defaults(
    items: &ItemTree,
    generics: &Generics,
    module: ModuleId,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) {
    for (i, param) in generics.params.iter().enumerate() {
        let Some((ty, range)) = &param.default else {
            continue;
        };
        let mut invalid = false;
        let mut check_path = |path: &Path| {
            if path.anchor != flux_hir::PathAnchor::Plain {
                return;
            }
            let Some(&name) = path.segments.first() else {
                return;
            };
            if name == kw::SelfUpper {
                errors.push((
                    *range,
                    "generic parameters cannot use `Self` in their defaults".into(),
                ));
                invalid = true;
            } else if generics.params.iter().skip(i).any(|p| p.name == name) {
                errors.push((*range, "generic parameter defaults cannot reference parameters before they are declared".into()));
                invalid = true;
            }
        };
        visit_paths(ty, &mut check_path);
        visit_default_const_paths(ty, &mut check_path);
        if !invalid {
            // Defaults resolve before the item's bounds: associated types
            // need a qualified path, as in rustc (E0220).
            let scope = TypeScope {
                bounds: &[],
                ..scope
            };
            let resolved =
                resolve_generic_arg(items, module, Some(param), ty, scope, *range, errors);
            if !param.is_const() {
                let names = generics.lifetimes.iter().map(|l| Some(l.name)).collect();
                let mut walker = regions::Walker::new(items, module, names);
                walker.scope = scope;
                walker.regions(ty, resolved);
                errors.extend(walker.errors);
            }
        }
    }
}

/// Constant paths are distinct from the type paths used for alias-cycle
/// analysis. Defaults must also reject forward references in these positions.
fn visit_default_const_paths(ty: &TypeRef, f: &mut impl FnMut(&Path)) {
    let mut constant = |len: &flux_hir::ArrayLen| {
        if let flux_hir::ArrayLenKind::Path(path) = &len.kind {
            f(path);
        }
    };
    match ty {
        TypeRef::Const(len) => constant(len),
        TypeRef::Array { elem, len } => {
            constant(len);
            visit_default_const_paths(elem, f);
        }
        TypeRef::Path(path) => {
            for arg in path.args.iter().flatten().flat_map(|a| a.types.iter()) {
                visit_default_const_paths(arg, f);
            }
            if let Some(q) = &path.qself {
                visit_default_const_paths(&q.ty, f);
            }
        }
        TypeRef::Tuple(fields) => {
            for field in fields {
                visit_default_const_paths(field, f);
            }
        }
        TypeRef::Ref { pointee, .. } | TypeRef::Ptr { pointee, .. } | TypeRef::Slice(pointee) => {
            visit_default_const_paths(pointee, f)
        }
        TypeRef::FnPtr { params, ret, .. } => {
            for param in params {
                visit_default_const_paths(param, f);
            }
            visit_default_const_paths(ret, f);
        }
        _ => {}
    }
}

/// Resolves a type as written in `module`: a struct, a primitive type,
/// a generic parameter, or `Self`. Errors go to `errors`, at the part of
/// the type they are about within `range`, and make the type an error
/// unless they are about privacy.
pub fn resolve_type(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let mutability = |mutable: bool| {
        if mutable {
            Mutability::Mut
        } else {
            Mutability::Not
        }
    };
    let fail = |errors: &mut Vec<(TextRange, String)>, at: TextRange, message: String| {
        errors.push((at, message));
        Ty::ERROR
    };
    match ty {
        TypeRef::Path(path) => {
            let last = path.segments.len().saturating_sub(1);
            // Generic arguments are only for the type itself, not the
            // modules before it.
            for i in 0..last {
                if let Some(args) = path.args(i) {
                    let name = path.segments[i];
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on module `{name}`"),
                    ));
                }
            }
            let args = path.args(last);
            // Arguments to something that takes none.
            let no_args = |errors: &mut Vec<(TextRange, String)>, what: String| {
                if let Some(args) = args {
                    errors.push((
                        args.ranges.first().copied().unwrap_or(args.range),
                        format!("type arguments are not allowed on {what}"),
                    ));
                }
            };
            if let Some(name) = path.as_name()
                && let Some(param) = scope.param(name)
            {
                if scope.const_param(name).is_some()
                    && !scope.type_param(name)
                    && !scope.consts.is_empty()
                {
                    return fail(
                        errors,
                        range,
                        format!("cannot find type `{name}` in this scope"),
                    );
                }
                no_args(errors, format!("type parameter `{name}`"));
                return param;
            }
            // `T::Out`, `Self::Out`, `<T as Tr>::Out`.
            let first = path.segments.first().copied();
            let relative = path.anchor == flux_hir::PathAnchor::Plain
                && path.segments.len() > 1
                && first.is_some_and(|f| scope.param(f).is_some() || f == kw::SelfUpper);
            if path.qself.is_some() || relative {
                return assoc_type(items, module, path, scope, range, errors);
            }
            let resolved = match items.resolve(module, path, Ns::Types) {
                Err(e @ ResolveError::Private { .. }) => {
                    let ResolveError::Private {
                        message, resolved, ..
                    } = &e
                    else {
                        unreachable!()
                    };
                    errors.push((e.range(path, range), message.clone()));
                    Ok(*resolved)
                }
                result => result,
            };
            let last_range = path.range(last, range);
            match resolved {
                Ok(Resolved::SelfType) => {
                    no_args(errors, "self type".to_owned());
                    match scope.self_ty {
                        Some(ty) => ty,
                        None if outer_generic(items, module, kw::SelfUpper) => {
                            fail(errors, range, "can't use `Self` from outer item".to_owned())
                        }
                        None => fail(
                            errors,
                            range,
                            "cannot find type `Self` in this scope".to_owned(),
                        ),
                    }
                }
                Ok(Resolved::Def(Def::Struct(id))) => {
                    struct_type(items, module, id, path, scope, range, errors)
                }
                Ok(Resolved::Def(Def::TypeAlias(id))) => {
                    alias_type(items, module, id, path, scope, range, errors)
                }
                Ok(Resolved::Primitive(name)) => {
                    no_args(errors, format!("builtin type `{name}`"));
                    match primitive(name) {
                        Ok(ty) => ty,
                        Err(message) => fail(errors, range, message),
                    }
                }
                Ok(Resolved::Def(Def::Trait(_)))
                    if matches!(
                        path.edition.unwrap_or_else(|| items.edition(module)),
                        flux_syntax::Edition::E2015 | flux_syntax::Edition::E2018
                    ) =>
                {
                    let object = flux_hir::DynRef {
                        bounds: Box::new([flux_hir::Bound {
                            bound_lifetimes: Box::default(),
                            path: path.clone(),
                            range,
                        }]),
                        lifetime: None,
                        range,
                    };
                    crate::objects::resolve_dyn(items, module, &object, scope, range, errors)
                }
                Ok(Resolved::Def(Def::Trait(_))) => fail(
                    errors,
                    last_range,
                    "expected a type, found a trait".to_owned(),
                ),
                Ok(Resolved::Def(Def::Module(m))) => {
                    // A module named `str` (including `use core::str`) can
                    // coexist with the primitive in a bare type position.
                    // Qualified module paths and differently named aliases
                    // still denote modules, and nominal types shadow builtins.
                    if let Some(name) = path.as_name()
                        && let Ok(ty) = primitive(name)
                    {
                        no_args(errors, format!("builtin type `{name}`"));
                        ty
                    } else {
                        fail(
                            errors,
                            last_range,
                            format!("expected type, found module `{}`", items.modules[m].name),
                        )
                    }
                }
                Ok(Resolved::Def(Def::Variant(..))) => fail(
                    errors,
                    range,
                    format!("expected type, found variant `{path}`"),
                ),
                Ok(_) => fail(errors, last_range, format!("expected type, found `{path}`")),
                Err(e @ ResolveError::NotFound { .. }) => {
                    let module_of_use = module;
                    let ResolveError::NotFound { segment, module } = e else {
                        unreachable!()
                    };
                    let name = path.segments[segment];
                    if module.is_none() && outer_generic(items, module_of_use, name) {
                        return fail(
                            errors,
                            e.range(path, range),
                            "can't use generic parameters from outer item".to_owned(),
                        );
                    }
                    let place = match module {
                        Some(module) => items.in_module(module),
                        None => "this scope".to_owned(),
                    };
                    fail(
                        errors,
                        e.range(path, range),
                        format!("cannot find type `{name}` in {place}"),
                    )
                }
                Err(ResolveError::Reported) => Ty::ERROR,
                Err(e) => {
                    let at = e.range(path, range);
                    let (ResolveError::Failed { message, .. }
                    | ResolveError::Private { message, .. }) = e
                    else {
                        unreachable!()
                    };
                    fail(errors, at, message)
                }
            }
        }
        TypeRef::Tuple(fields) => {
            let fields: Vec<Ty> = fields
                .iter()
                .map(|f| resolve_type(items, module, f, scope, range, errors))
                .collect();
            if fields.contains(&Ty::ERROR) {
                return Ty::ERROR;
            }
            // Only the last element may be unsized, as a struct's last
            // field may.
            let leading = &fields[..fields.len().saturating_sub(1)];
            if let Some(&field) = leading.iter().find(|&&f| scope.is_unsized(items, f)) {
                return fail(errors, range, unsized_error(field));
            }
            Ty::tuple(&fields)
        }
        TypeRef::Never => Ty::NEVER,
        TypeRef::FnPtr {
            bound_lifetimes,
            params,
            ret,
            is_unsafe,
            abi,
        } => {
            let scope = TypeScope {
                in_fn_pointer: true,
                ..scope
            };
            let params: Vec<Ty> = params
                .iter()
                .map(|p| resolve_type(items, module, p, scope, range, errors))
                .collect();
            let ret = resolve_type(items, module, ret, scope, range, errors);
            if params.contains(&Ty::ERROR) || ret == Ty::ERROR {
                return Ty::ERROR;
            }
            let TypeRef::FnPtr {
                params: written,
                ret: written_ret,
                ..
            } = ty
            else {
                unreachable!()
            };
            let signature = FnSigTy {
                abi: *abi,
                ..FnSigTy::new(&params, ret, *is_unsafe)
            };
            let description = fn_regions::describe(
                items,
                module,
                bound_lifetimes,
                written,
                written_ret,
                signature,
                errors,
            );
            Ty::new(TyKind::FnPtr(FnSigTy {
                regions: Some(description.regions),
                ..signature
            }))
        }
        TypeRef::Ptr { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::raw_ptr(mutability(*mutable), pointee)
        }
        TypeRef::Ref {
            mutable, pointee, ..
        } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::Ref(mutability(*mutable), pointee))
        }
        TypeRef::Array { elem, len } => {
            let elem = resolve_type(items, module, elem, scope, range, errors);
            let len = array_len(items, module, len, scope, errors);
            match (elem, len) {
                (Ty::ERROR, _) | (_, None) => Ty::ERROR,
                (elem, _) if scope.is_unsized(items, elem) => {
                    fail(errors, range, unsized_error(elem))
                }
                (elem, Some(len)) => Ty::array_of(elem, len),
            }
        }
        TypeRef::Const(_) => fail(
            errors,
            range,
            "constant provided when a type was expected".to_owned(),
        ),
        TypeRef::Slice(elem) => {
            let elem = resolve_type(items, module, elem, scope, range, errors);
            if elem == Ty::ERROR {
                return Ty::ERROR;
            }
            if scope.is_unsized(items, elem) {
                return fail(errors, range, unsized_error(elem));
            }
            Ty::slice(elem)
        }
        TypeRef::ImplParam(index) => match scope.generics.get(*index as usize) {
            Some(&name) => Ty::param(*index, name),
            None => Ty::ERROR,
        },
        TypeRef::Opaque(index) => {
            let Some(function) = scope.function else {
                return Ty::ERROR;
            };
            let Some(decl) = items.functions[function].opaques.get(*index as usize) else {
                return Ty::ERROR;
            };
            Ty::opaque(OpaqueTy {
                function,
                index: *index,
                name: decl.name,
                args: Ty::list(&params_of_names(scope.generics)),
                lifetimes: 0,
            })
        }
        TypeRef::Dyn(d) => objects::resolve_dyn(items, module, d, scope, range, errors),
        // One lowering did not make something else, which is in a path
        // in a body (`Option::<impl Copy>::None`), as rustc names it.
        TypeRef::ImplTrait(it) => fail(
            errors,
            it.range,
            "`impl Trait` is not allowed in paths".to_owned(),
        ),
        TypeRef::Infer => match scope.signature_of {
            Some(items) => fail(
                errors,
                range,
                format!(
                    "the placeholder `_` is not allowed within types on item signatures for {items}"
                ),
            ),
            None => Ty::infer(ty::InferVar::PLACEHOLDER),
        },
        TypeRef::Error => Ty::ERROR,
    }
}

/// The value of an array's length, `len`, written in `module`: a
/// constant of type `usize` ([`const_value`]).
pub fn array_len(
    items: &ItemTree,
    module: ModuleId,
    len: &flux_hir::ArrayLen,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<Ty> {
    const_value(items, module, len, Ty::int(IntTy::Usize), scope, errors)
}

/// The value of a constant written in a type, `len` (an array's length
/// or a const generic argument), of type `want`, in `module`: a
/// [`TyKind::Value`], or a const parameter. `None` if it is not known
/// (reported unless it is waiting for evaluation).
pub fn const_value(
    items: &ItemTree,
    module: ModuleId,
    len: &flux_hir::ArrayLen,
    want: Ty,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<Ty> {
    let range = len.range;
    let mismatch = |errors: &mut Vec<(TextRange, String)>, found: &dyn std::fmt::Display| {
        errors.push((
            range,
            format!("mismatched types: expected `{want}`, found {found}"),
        ));
        None
    };
    let value = |bits: u128| Some(Ty::value(want, bits));
    let literal_overflow = |errors: &mut Vec<(TextRange, String)>| {
        let file = scope.function.map_or(items.modules[module].file, |id| {
            items.functions[id].loc.file
        });
        let message = format!("literal out of range for `{want}`");
        match items.lint_level_at(module, file, range, "overflowing_literals") {
            flux_hir::LintLevel::Allow => false,
            flux_hir::LintLevel::Expect => {
                items.fulfill_overflow_expectation_at(module, file, range);
                false
            }
            flux_hir::LintLevel::Warn => {
                items.record_lint_warning(Diagnostic {
                    file,
                    range,
                    message,
                });
                false
            }
            flux_hir::LintLevel::Deny | flux_hir::LintLevel::Forbid => {
                errors.push((range, message));
                true
            }
        }
    };
    let konst = match &len.kind {
        flux_hir::ArrayLenKind::Error => return None,
        flux_hir::ArrayLenKind::Const(konst) => *konst,
        flux_hir::ArrayLenKind::NegLit {
            value: magnitude,
            suffix,
        } => {
            let Some(int) = want.as_int() else {
                return mismatch(errors, &"integer");
            };
            if let Some(suffix) = suffix
                && *suffix != int.name()
            {
                return mismatch(errors, &format!("`{suffix}`"));
            }
            if !int.is_signed() {
                errors.push((
                    range,
                    format!("cannot apply unary operator `-` to type `{want}`"),
                ));
                return None;
            }
            if *magnitude > int.max() + 1 && literal_overflow(errors) {
                return None;
            }
            let mask = u128::MAX >> (128 - int.bits());
            return value(magnitude.wrapping_neg() & mask);
        }
        flux_hir::ArrayLenKind::Anon(file, node) => {
            // One that uses generic parameters is reported, and has none.
            let konst = *items.anon_consts.get(&(*file, *node))?;
            let lens = scope.lens?;
            if items.consts[konst].anon.is_some_and(|a| a.is_arg) {
                lens.found_arg_ty(konst, want);
            }
            return lens.value(konst).and_then(value);
        }
        flux_hir::ArrayLenKind::Lit {
            value: bits,
            suffix,
        } => {
            let Some(int) = want.as_int() else {
                if want == Ty::ERROR {
                    return None;
                }
                return mismatch(errors, &"integer");
            };
            return match suffix {
                Some(suffix) if *suffix != int.name() => mismatch(errors, &format!("`{suffix}`")),
                _ if *bits > int.max() && literal_overflow(errors) => None,
                _ => value(*bits & (u128::MAX >> (128 - int.bits()))),
            };
        }
        flux_hir::ArrayLenKind::Bool(b) => {
            if want != Ty::BOOL {
                return mismatch(errors, &"`bool`");
            }
            return value(u128::from(*b));
        }
        flux_hir::ArrayLenKind::Char(c) => {
            if want != Ty::CHAR {
                return mismatch(errors, &"`char`");
            }
            return value(u128::from(*c));
        }
        flux_hir::ArrayLenKind::Path(path)
            if let Some(name) = path.as_name()
                && let Some(ty) = scope.const_param(name) =>
        {
            if scope.type_param(name) {
                errors.push((
                    range,
                    format!("expected value, found type parameter `{name}`"),
                ));
                return None;
            }
            if ty != want && ty != Ty::ERROR && want != Ty::ERROR {
                return mismatch(errors, &format!("`{ty}`"));
            }
            return scope.param(name);
        }
        flux_hir::ArrayLenKind::Path(path)
            if path.segments.first() == Some(&kw::SelfUpper)
                && scope.self_ty.is_some_and(Ty::has_params) =>
        {
            errors.push((
                path.range(0, range),
                "generic `Self` types are currently not permitted in anonymous constants"
                    .to_owned(),
            ));
            return None;
        }
        flux_hir::ArrayLenKind::Path(path) => match items.resolve(module, path, Ns::Values) {
            Ok(Resolved::Def(Def::Const(konst))) => konst,
            // A constant of an `impl` block without generic parameters.
            Ok(Resolved::AssocConst(konst))
                if items.consts[konst]
                    .owner
                    .and_then(Owner::as_impl)
                    .is_some_and(|imp| items.impls[imp].generics.is_empty()) =>
            {
                konst
            }
            Err(ResolveError::Reported) => return None,
            Err(ResolveError::NotFound { segment, .. }) if path.segments.len() == 1 => {
                let name = path.segments[segment];
                errors.push((range, format!("cannot find value `{name}` in this scope")));
                return None;
            }
            _ => {
                errors.push((
                    range,
                    "array lengths other than integers and constants are not supported yet"
                        .to_owned(),
                ));
                return None;
            }
        },
    };
    let data = &items.consts[konst];
    let ty = resolve_type(
        items,
        data.loc.module,
        &data.ty,
        TypeScope::default(),
        data.ty_range,
        &mut Vec::new(),
    );
    if ty != want {
        if ty == Ty::ERROR || want == Ty::ERROR {
            return None;
        }
        return mismatch(errors, &format!("`{ty}`"));
    }
    scope.lens?.value(konst).and_then(value)
}

/// Whether `name` is a generic parameter, or `Self`, of a function whose
/// body has the block `module` is, or is in: which an item declared
/// there cannot use (E0401).
fn outer_generic(items: &ItemTree, module: ModuleId, name: Symbol) -> bool {
    let mut module = Some(module);
    while let Some(m) = module {
        let data = &items.modules[m];
        module = data.parent;
        let Some(f) = data.block.and_then(|b| b.function) else {
            continue;
        };
        let f = &items.functions[f];
        let found = if name == kw::SelfUpper {
            f.owner.is_some()
        } else {
            generic_names(items, f.owner, &f.generics).contains(&name)
        };
        if found {
            return true;
        }
    }
    false
}

/// An associated type named by a path: `T::Out`, `Self::Out`,
/// `<T as Tr>::Out` or `<T>::Out`.
fn assoc_type(
    items: &ItemTree,
    module: ModuleId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let base = assoc_type_base(items, module, path, scope, range, errors);
    let TyKind::Alias(alias) = base.kind() else {
        return base;
    };
    if scope.in_fn_pointer && path.qself.is_none() && regions::has_projection_inputs(items, alias) {
        errors.push((
            range,
            "associated family regions in function pointers are not supported yet".into(),
        ));
        return Ty::ERROR;
    }
    let index = path.segments.len() - 1;
    resolve_family_arguments(
        items,
        module,
        alias,
        path.args(index),
        scope,
        path.range(index, range),
        errors,
    )
    .map(Ty::alias)
    .unwrap_or(Ty::ERROR)
}

/// The family's own argument list, shared by projection paths and equality
/// bindings. Trait membership remains exclusively in `alias.args`.
pub(crate) fn resolve_family_arguments(
    items: &ItemTree,
    module: ModuleId,
    mut alias: crate::AliasTy,
    written: Option<&flux_hir::GenericArgs>,
    scope: TypeScope<'_>,
    at: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Option<crate::AliasTy> {
    let Some(declaration) = items.traits[alias.trait_]
        .types
        .iter()
        .find(|ty| ty.name == alias.name)
    else {
        return Some(alias);
    };
    let want = declaration.generics.params.len();
    let got = written.map_or(0, |args| args.types.len());
    if want != got {
        errors.push((at, wrong_arg_count("associated type", want, got)));
        return None;
    }
    let lifetimes = declaration.generics.lifetimes.len();
    let given_lifetimes = written.map_or(0, |args| args.lifetimes.len());
    if given_lifetimes != lifetimes {
        errors.push((
            written.map_or(at, |args| args.range),
            format!("expected {lifetimes} lifetime arguments, found {given_lifetimes}"),
        ));
        return None;
    }
    if let Some(written) = written {
        if written.parenthesized || !written.bindings.is_empty() || !written.bounds.is_empty() {
            errors.push((
                written.range,
                "associated type arguments must be types, constants or lifetimes".into(),
            ));
            return None;
        }
        alias.family_args = Ty::list(&resolve_generic_args(
            items,
            module,
            &declaration.generics.params,
            written,
            scope,
            errors,
        ));
        if let Some((_, &arg)) = declaration
            .generics
            .params
            .iter()
            .zip(alias.family_args)
            .find(|(param, arg)| !param.maybe_unsized && scope.is_unsized(items, **arg))
        {
            errors.push((at, unsized_error(arg)));
            return None;
        }
    }
    Some(alias)
}

fn assoc_type_base(
    items: &ItemTree,
    module: ModuleId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let (self_ty, trait_path, name_index) = match &path.qself {
        Some(qself) => {
            let ty = resolve_type(items, module, &qself.ty, scope, qself.ty_range, errors);
            (ty, qself.trait_.as_ref().map(|t| (t, qself.range)), 0)
        }
        None => {
            let first = Path {
                segments: path.segments[..1].into(),
                ranges: path.ranges[..1.min(path.ranges.len())].into(),
                args: path.args[..1.min(path.args.len())].into(),
                ..path.clone()
            };
            let ty = resolve_type(items, module, &TypeRef::Path(first), scope, range, errors);
            (ty, None, 1)
        }
    };
    if path.segments.len() != name_index + 1 {
        errors.push((
            range,
            "associated types of associated types are not supported yet".into(),
        ));
        return Ty::ERROR;
    }
    if self_ty == Ty::ERROR {
        return Ty::ERROR;
    }
    let name = path.segments[name_index];
    let at = path.range(name_index, range);
    let has_type = |t: flux_hir::TraitId| items.traits[t].types.iter().any(|ty| ty.name == name);
    let captures_inputs = |t: flux_hir::TraitId| {
        !items.traits[t].generics.lifetimes.is_empty()
            || items.traits[t]
                .types
                .iter()
                .find(|ty| ty.name == name)
                .is_some_and(|ty| {
                    !ty.generics.params.is_empty() || !ty.generics.lifetimes.is_empty()
                })
    };
    // `<T as Tr>::Out`: the trait is written.
    if let Some((trait_path, qrange)) = trait_path {
        let bound = flux_hir::Bound {
            bound_lifetimes: Box::default(),
            path: trait_path.clone(),
            range: qrange,
        };
        let Some((mut trait_ref, _)) =
            traits::resolve_bound(items, module, &bound, self_ty, scope, errors)
        else {
            return Ty::ERROR;
        };
        if !has_type(trait_ref.trait_)
            && path.qself.as_ref().is_some_and(|q| q.associated_bound)
            && let Some(owner) = traits::supertrait_with_type(items, trait_ref.trait_, name)
        {
            trait_ref = TraitRef {
                trait_: owner,
                name: items.traits[owner].name,
                args: Ty::list(
                    &trait_ref.args[..items.traits[owner].generics.len().min(trait_ref.args.len())],
                ),
            };
        }
        if !has_type(trait_ref.trait_) {
            errors.push((
                at,
                format!(
                    "cannot find associated type `{name}` in trait `{}`",
                    trait_ref.name
                ),
            ));
            return Ty::ERROR;
        }
        let self_written = path.qself.as_ref().is_some_and(
            |q| matches!(&q.ty, TypeRef::Path(p) if p.as_name() == Some(kw::SelfUpper)),
        );
        if self_written
            && !captures_inputs(trait_ref.trait_)
            && Some(trait_ref) == scope.self_trait
            && let Some(&(_, ty)) = scope.self_types.iter().find(|(n, _)| *n == name)
        {
            return ty;
        }
        // AliasTy has no trait-region table. Reject these projections until
        // normalization can preserve their owner and type-argument regions.
        if !captures_inputs(trait_ref.trait_)
            && trait_ref
                .args
                .iter()
                .any(|&t| regions::slots(items, t) != 0)
        {
            errors.push((
                qrange,
                "lifetimes in qualified trait projections are not supported yet".into(),
            ));
            return Ty::ERROR;
        }
        return Ty::alias(flux_solve::traits::alias_of(
            items, trait_ref, self_ty, name,
        ));
    }
    // `Self::Out` in a trait's `impl` block: the block's own.
    // (Only written `Self`: `I::Item` in `impl<I: It> Tr for I` is `I`'s
    // bounds', as in rustc.)
    let written_self = path.qself.is_none() && path.segments[0] == kw::SelfUpper;
    if let Some(trait_ref) = scope.self_trait
        && Some(self_ty) == scope.self_ty
        && (written_self || !matches!(self_ty.kind(), TyKind::Param(_)))
    {
        if has_type(trait_ref.trait_) {
            if written_self
                && !captures_inputs(trait_ref.trait_)
                && let Some(&(_, ty)) = scope.self_types.iter().find(|(n, _)| *n == name)
            {
                return ty;
            }
            return Ty::alias(flux_solve::traits::alias_of(
                items, trait_ref, self_ty, name,
            ));
        }
        if written_self {
            let found: Vec<_> = scope
                .self_supertraits
                .iter()
                .filter(|p| has_type(p.trait_ref.trait_))
                .map(|p| p.trait_ref)
                .collect();
            match found.as_slice() {
                [inherited] => {
                    // A closed, region-free definition cannot capture the
                    // borrowed inputs. Its impl requirements are proved by
                    // namespace lookup and checked with regions separately.
                    if !captures_inputs(inherited.trait_)
                        && let Some(&(_, _, ty)) = scope
                            .self_inherited_types
                            .iter()
                            .find(|(tr, n, _)| tr == inherited && *n == name)
                        && regions::is_closed_region_free(items, ty)
                    {
                        return ty;
                    }
                    // AliasTy has no table for erased trait-argument regions.
                    // Preserve the qualified-projection boundary here too.
                    if !captures_inputs(inherited.trait_)
                        && inherited
                            .args
                            .iter()
                            .any(|&t| regions::slots(items, t) != 0)
                    {
                        return Ty::alias(flux_solve::traits::alias_of(
                            items, *inherited, self_ty, name,
                        ));
                    }
                    if captures_inputs(inherited.trait_) {
                        return Ty::alias(flux_solve::traits::alias_of(
                            items, *inherited, self_ty, name,
                        ));
                    }
                    if let Some(&(_, _, ty)) = scope
                        .self_inherited_types
                        .iter()
                        .find(|(tr, n, _)| tr == inherited && *n == name)
                    {
                        fn has_regions(items: &ItemTree, ty: Ty) -> bool {
                            regions::slots(items, ty) != 0
                                || ty.children().any(|t| has_regions(items, t))
                        }
                        if has_regions(items, ty)
                            && (scope.in_fn_pointer || type_has_pointer_binder(ty))
                        {
                            errors.push((at, "inherited associated type regions in function pointers are not supported yet".into()));
                            return Ty::ERROR;
                        }
                        if ty.has_aliases() {
                            // Keep the declaring supertrait's projection. Its
                            // selected impl may forward through another alias;
                            // normalization must retain that header selection
                            // and its lifetime recipe, rather than exposing an
                            // erased RHS under the spelling `Self::Out`.
                            return Ty::alias(flux_solve::traits::alias_of(
                                items, *inherited, self_ty, name,
                            ));
                        }
                        if scope.signature_of != Some("functions") && regions::slots(items, ty) != 0
                        {
                            errors.push((at, "region-bearing inherited associated types outside function signatures are not supported yet".into()));
                            return Ty::ERROR;
                        }
                        return ty;
                    }
                    return Ty::alias(flux_solve::traits::alias_of(
                        items, *inherited, self_ty, name,
                    ));
                }
                [] => errors.push((at, format!("associated type `{name}` not found for `Self`"))),
                _ => errors.push((at, format!("ambiguous associated type `{name}` for `Self`"))),
            }
            return Ty::ERROR;
        }
    }
    if !matches!(
        self_ty.kind(),
        TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)
    ) {
        errors.push((range, "ambiguous associated type".into()));
        return Ty::ERROR;
    }
    // `T::Out`: from the one trait among its bounds' with a type `Out`.
    let mut found: Vec<TraitRef> = Vec::new();
    for p in scope.bounds {
        if p.self_ty == self_ty
            && has_type(p.trait_ref.trait_)
            && !found.iter().any(|t| t.trait_ == p.trait_ref.trait_)
        {
            found.push(p.trait_ref);
        }
    }
    match found.as_slice() {
        [trait_ref] => Ty::alias(flux_solve::traits::alias_of(
            items, *trait_ref, self_ty, name,
        )),
        [] => {
            errors.push((
                at,
                format!("associated type `{name}` not found for `{self_ty}`"),
            ));
            Ty::ERROR
        }
        _ => {
            errors.push((
                range,
                format!("ambiguous associated type `{name}` in bounds of `{self_ty}`"),
            ));
            Ty::ERROR
        }
    }
}

/// The bounds of `generics`, as [`traits::resolve_predicates`]; `T::Out`
/// in one may name the associated type of a bound on `T` in another, so
/// if any failed, they are resolved again with the others in scope.
pub(super) fn resolve_predicates(
    items: &ItemTree,
    sigs: &Signatures,
    module: ModuleId,
    generics: &Generics,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> traits::Bounds {
    let mut pending_errors = Vec::new();
    let mut resolved =
        traits::resolve_predicates(items, module, generics, scope, &mut pending_errors);
    // A where-clause may use a later bound's associated type. Resolve one
    // dependency layer per pass, rather than assuming two passes suffice.
    for _ in 0..generics.predicates.len() {
        if pending_errors.is_empty() {
            return resolved;
        }
        let all: Vec<Predicate> = scope
            .bounds
            .iter()
            .chain(&resolved.predicates)
            .copied()
            .collect();
        let bounds = flux_solve::traits::elaborate(sigs, &all);
        pending_errors.clear();
        let next = traits::resolve_predicates(
            items,
            module,
            generics,
            TypeScope {
                bounds: &bounds,
                ..scope
            },
            &mut pending_errors,
        );
        let unchanged =
            next.predicates == resolved.predicates && next.projections == resolved.projections;
        resolved = next;
        if unchanged {
            break;
        }
    }
    errors.extend(pending_errors);
    resolved
}

/// Detect a pointer binder in a carrier declaration before its arguments
/// erase the provenance of an inherited projection. Cycles are visited once;
/// excessive declaration depth is a conservative binder boundary.
fn declaration_has_pointer_binder(items: &ItemTree, def: Def, seen: &mut Vec<Def>) -> bool {
    if seen.contains(&def) {
        return false;
    }
    if seen.len() >= 128 {
        return true;
    }
    seen.push(def);
    match def {
        Def::Struct(id) => {
            let data = &items.structs[id];
            if data
                .generics
                .params
                .iter()
                .filter_map(|p| p.default.as_ref())
                .any(|(ty, _)| written_has_pointer_binder(items, data.loc.module, ty, seen))
            {
                return true;
            }
            (0..data.variant_count() as u32).any(|v| {
                data.variant(v)
                    .fields
                    .iter()
                    .any(|f| written_has_pointer_binder(items, data.loc.module, &f.ty, seen))
            })
        }
        Def::TypeAlias(id) => {
            let data = &items.type_aliases[id];
            written_has_pointer_binder(items, data.loc.module, &data.ty, seen)
        }
        _ => false,
    }
}

fn written_has_pointer_binder(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    seen: &mut Vec<Def>,
) -> bool {
    match ty {
        TypeRef::FnPtr { .. } => true,
        TypeRef::Ref { pointee, .. }
        | TypeRef::Ptr { pointee, .. }
        | TypeRef::Slice(pointee)
        | TypeRef::Array { elem: pointee, .. } => {
            written_has_pointer_binder(items, module, pointee, seen)
        }
        TypeRef::Tuple(ts) => ts
            .iter()
            .any(|t| written_has_pointer_binder(items, module, t, seen)),
        TypeRef::Path(path) => {
            if path.qself.is_some() {
                return true;
            }
            if path
                .args
                .iter()
                .flatten()
                .flat_map(|a| a.types.iter())
                .any(|t| written_has_pointer_binder(items, module, t, seen))
            {
                return true;
            }
            let resolved = match items.resolve(module, path, Ns::Types) {
                Ok(r) | Err(ResolveError::Private { resolved: r, .. }) => Some(r),
                _ => None,
            };
            matches!(resolved, Some(Resolved::Def(def)) if declaration_has_pointer_binder(items, def, seen))
        }
        _ => false,
    }
}

fn type_has_pointer_binder(ty: Ty) -> bool {
    // Nominal fields are checked in their declaration context. A pointer in
    // an ADT's fields is not a binder surrounding this occurrence of Self::Out;
    // only the resolved value type and its generic arguments can be one.
    matches!(ty.kind(), TyKind::FnPtr(_)) || ty.children().any(type_has_pointer_binder)
}

/// Struct `strukt`, named by `path`, with the generic arguments written
/// after its last segment, which must be one for each of its
/// parameters.
pub fn struct_type(
    items: &ItemTree,
    module: ModuleId,
    strukt: StructId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let def = &items.structs[strukt];
    let scope = TypeScope {
        in_fn_pointer: scope.in_fn_pointer
            || (!scope.self_inherited_types.is_empty()
                && declaration_has_pointer_binder(items, Def::Struct(strukt), &mut Vec::new())),
        ..scope
    };
    // `core::ffi::CStr` is the type of `c"..."`, which the compiler
    // knows.
    if items.lang_enums.get(&Symbol::intern("CStr")) == Some(&strukt) {
        return Ty::new(TyKind::CStr);
    }
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    for args in path.args.iter().flatten() {
        for bound in &args.bounds {
            errors.push((bound.range, flux_hir::MISPLACED_BINDING.into()));
        }
        for b in args.bindings.iter() {
            errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
        }
    }
    let none;
    let written = match path.args(last) {
        Some(written) => written,
        None if want == 0 => return Ty::adt(strukt, def.name, &[]),
        // `S` for `struct S<T = i32>` is `S<i32>`.
        None if def.generics.params[0].default.is_some() => {
            none = flux_hir::GenericArgs {
                bounds: Box::default(),
                lifetimes: Box::new([]),
                types: Box::new([]),
                ranges: Box::new([]),
                bindings: Box::new([]),
                range: at,
                parenthesized: false,
                bound_lifetimes: Box::default(),
            };
            &none
        }
        None => {
            let kind = if def.is_enum() { "enum" } else { "struct" };
            errors.push((at, format!("missing generics for {kind} `{}`", def.name)));
            return Ty::ERROR;
        }
    };
    let got = written.types.len();
    let kind = if def.is_enum() { "enum" } else { "struct" };
    if let Some(message) = type_arg_count(kind, &def.generics.params, got) {
        errors.push((at, message));
        return Ty::ERROR;
    }
    let mut args =
        resolve_generic_args(items, module, &def.generics.params, written, scope, errors);
    if args.len() != got {
        let kind = if def.is_enum() { "enum" } else { "struct" };
        errors.push((at, wrong_arg_count(kind, want, args.len())));
        return Ty::ERROR;
    }
    fill_type_defaults(items, Def::Struct(strukt), &mut args, scope, errors);
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    let params = &def.generics.params;
    if let Some((_, &arg)) = args
        .iter()
        .enumerate()
        .find(|&(i, &a)| !params[i].maybe_unsized && scope.is_unsized(items, a))
    {
        // At the path and its arguments, as rustc points.
        let at = match (path.ranges.first(), path.args(last)) {
            (Some(first), Some(args)) if first.start() <= args.range.end() => {
                TextRange::new(first.start(), args.range.end())
            }
            _ => range,
        };
        errors.push((at, unsized_error(arg)));
        return Ty::ERROR;
    }
    let ty = Ty::adt(strukt, def.name, &args);
    let tail = metadata::tail(scope.struct_fields, ty, |id| items.structs[id].is_enum());
    if matches!(tail.kind(), TyKind::Dynamic(_))
        && scope
            .struct_fields
            .and_then(|fields| metadata::transparent_object_tail(items, fields, ty))
            .is_none()
    {
        errors.push((
            at,
            "trait object tails in structs are not supported yet".into(),
        ));
        return Ty::ERROR;
    }
    ty
}

/// A nominal type's implicit Sized arguments still apply through aliases and
/// forward field declarations after their structural metadata is available.
pub(super) fn unsized_argument(items: &ItemTree, scope: TypeScope<'_>, ty: Ty) -> Option<Ty> {
    let mut stack = vec![ty];
    while let Some(ty) = stack.pop() {
        if let TyKind::Adt(adt, args) = ty.kind() {
            for (param, &arg) in items.structs[adt.strukt].generics.params.iter().zip(args) {
                if !param.maybe_unsized && scope.is_unsized(items, arg) {
                    return Some(arg);
                }
            }
        }
        stack.extend(ty.children());
    }
    None
}

/// rustc's E0277 for a value of unsized type `ty`.
pub fn unsized_error(mut ty: Ty) -> String {
    // rustc names a tuple's unsized last element.
    while let TyKind::Tuple([.., last]) = ty.kind() {
        ty = *last;
    }
    format!("the size for values of type `{ty}` cannot be known at compilation time")
}

thread_local! {
    /// The type aliases being expanded, innermost last: one met again
    /// is a cycle, reported where the alias is checked.
    static EXPANDING: std::cell::RefCell<Vec<flux_hir::TypeAliasId>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Type alias `alias`, named by `path`, with the generic arguments
/// written after its last segment: the type it stands for, with them
/// for its parameters. Errors in the alias itself are reported where it
/// is checked, not at each use.
pub fn alias_type(
    items: &ItemTree,
    module: ModuleId,
    alias: flux_hir::TypeAliasId,
    path: &Path,
    scope: TypeScope<'_>,
    range: TextRange,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let def = &items.type_aliases[alias];
    let scope = TypeScope {
        in_fn_pointer: scope.in_fn_pointer
            || (!scope.self_inherited_types.is_empty()
                && declaration_has_pointer_binder(items, Def::TypeAlias(alias), &mut Vec::new())),
        ..scope
    };
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    for args in path.args.iter().flatten() {
        for bound in &args.bounds {
            errors.push((bound.range, flux_hir::MISPLACED_BINDING.into()));
        }
        for b in args.bindings.iter() {
            errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
        }
    }
    let mut args: Vec<Ty> = match path.args(last) {
        None if want == 0 => Vec::new(),
        None if def.generics.params[0].default.is_some() => Vec::new(),
        None => {
            errors.push((
                at,
                format!("missing generics for type alias `{}`", def.name),
            ));
            return Ty::ERROR;
        }
        Some(written) => {
            let args =
                resolve_generic_args(items, module, &def.generics.params, written, scope, errors);
            if let Some(message) = type_arg_count("type alias", &def.generics.params, args.len()) {
                errors.push((at, message));
                return Ty::ERROR;
            }
            args
        }
    };
    fill_type_defaults(items, Def::TypeAlias(alias), &mut args, scope, errors);
    let ty = expand_alias(items, alias, scope.lens, scope.defaults, &mut Vec::new());
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    let ty = ty.subst(&args);
    if let Some(arg) = unsized_argument(items, scope, ty) {
        errors.push((at, unsized_error(arg)));
        return Ty::ERROR;
    }
    ty
}

/// The type alias `alias` stands for, in terms of its parameters, with
/// the errors in it; an error if it is part of a cycle of aliases.
pub(crate) fn expand_alias(
    items: &ItemTree,
    alias: flux_hir::TypeAliasId,
    lens: Option<&ArrayLens<'_>>,
    defaults: Option<&crate::traits::TraitDefaults>,
    errors: &mut Vec<(TextRange, String)>,
) -> Ty {
    let cycle = EXPANDING.with(|stack| {
        let mut stack = stack.borrow_mut();
        if stack.contains(&alias) {
            return true;
        }
        stack.push(alias);
        false
    });
    if cycle {
        return Ty::ERROR;
    }
    let def = &items.type_aliases[alias];
    let generics: Vec<Symbol> = def.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        generics: &generics,
        signature_of: Some("type aliases"),
        lens,
        defaults,
        ..TypeScope::default()
    };
    let ty = resolve_type(items, def.loc.module, &def.ty, scope, def.ty_range, errors);
    EXPANDING.with(|stack| stack.borrow_mut().pop());
    ty
}

/// Whether expanding type alias `alias` meets `alias` again.
pub(super) fn alias_cycle(items: &ItemTree, alias: flux_hir::TypeAliasId) -> bool {
    let def = &items.type_aliases[alias];
    let mut found = false;
    visit_paths(&def.ty, &mut |path: &Path| {
        if !found
            && path.qself.is_none()
            && !(path.anchor == flux_hir::PathAnchor::Plain
                && path
                    .segments
                    .first()
                    .is_some_and(|name| def.generics.params.iter().any(|p| p.name == *name)))
            && let Ok(Resolved::Def(Def::TypeAlias(next))) =
                items.resolve(def.loc.module, path, Ns::Types)
        {
            found = reaches_alias(items, next, alias, &mut Vec::new());
        }
    });
    found
}

/// Whether type alias `from` names `target`, through other aliases.
pub(super) fn reaches_alias(
    items: &ItemTree,
    from: flux_hir::TypeAliasId,
    target: flux_hir::TypeAliasId,
    seen: &mut Vec<flux_hir::TypeAliasId>,
) -> bool {
    if from == target {
        return true;
    }
    if seen.contains(&from) {
        return false;
    }
    seen.push(from);
    let def = &items.type_aliases[from];
    let mut found = false;
    visit_paths(&def.ty, &mut |path: &Path| {
        if !found
            && path.qself.is_none()
            && !(path.anchor == flux_hir::PathAnchor::Plain
                && path
                    .segments
                    .first()
                    .is_some_and(|name| def.generics.params.iter().any(|p| p.name == *name)))
            && let Ok(Resolved::Def(Def::TypeAlias(next))) =
                items.resolve(def.loc.module, path, Ns::Types)
        {
            found = reaches_alias(items, next, target, seen);
        }
    });
    found
}

/// Calls `f` on every path in `ty`, generic arguments included.
fn visit_paths(ty: &TypeRef, f: &mut impl FnMut(&Path)) {
    match ty {
        TypeRef::Path(path) => {
            f(path);
            for args in path.args.iter().flatten() {
                for arg in args.types.iter() {
                    visit_paths(arg, f);
                }
                for binding in &args.bindings {
                    visit_paths(&binding.ty, f);
                }
            }
            if let Some(qself) = &path.qself {
                visit_paths(&qself.ty, f);
                for args in qself
                    .trait_
                    .iter()
                    .flat_map(|path| path.args.iter().flatten())
                {
                    for arg in &args.types {
                        visit_paths(arg, f);
                    }
                    for binding in &args.bindings {
                        visit_paths(&binding.ty, f);
                    }
                }
            }
        }
        TypeRef::Tuple(elements) => elements.iter().for_each(|e| visit_paths(e, f)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Ref { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => visit_paths(pointee, f),
        TypeRef::FnPtr { params, ret, .. } => params
            .iter()
            .chain([&**ret])
            .for_each(|t| visit_paths(t, f)),
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    args.types.iter().for_each(|t| visit_paths(t, f));
                    args.bindings.iter().for_each(|b| visit_paths(&b.ty, f));
                }
            }
        }
        TypeRef::Never
        | TypeRef::Infer
        | TypeRef::Const(_)
        | TypeRef::ImplTrait(_)
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Error => {}
    }
}

/// The primitive type called `name`.
pub fn primitive(name: Symbol) -> Result<Ty, String> {
    if name == sym::bool {
        return Ok(Ty::BOOL);
    }
    if name == sym::char {
        return Ok(Ty::CHAR);
    }
    if name == sym::str {
        return Ok(Ty::STR);
    }
    if let Some(float) = FloatTy::from_name(name) {
        return Ok(Ty::float(float));
    }
    match IntTy::from_name(name) {
        Some(int) => Ok(Ty::int(int)),
        None => Err(format!("`{name}` is not supported yet")),
    }
}

/// The `impl` blocks' type for methods on `ty`, if it has any.
pub fn impl_target(items: &ItemTree, ty: Ty) -> Option<ImplTarget> {
    match ty.kind() {
        TyKind::Adt(adt, _) => Some(ImplTarget::Struct(adt.strukt)),
        // `core`'s `impl CStr` block.
        TyKind::CStr => items
            .lang_enums
            .get(&Symbol::intern("CStr"))
            .map(|&s| ImplTarget::Struct(s)),
        TyKind::Int(int) => Some(ImplTarget::Primitive(int.name())),
        TyKind::Float(float) => Some(ImplTarget::Primitive(float.name())),
        TyKind::Bool => Some(ImplTarget::Primitive(sym::bool)),
        TyKind::Char => Some(ImplTarget::Primitive(sym::char)),
        TyKind::Str => Some(ImplTarget::Primitive(sym::str)),
        TyKind::Slice(_) => Some(ImplTarget::Primitive(flux_hir::slice_target())),
        TyKind::Array(..) => Some(ImplTarget::Primitive(flux_hir::array_target())),
        TyKind::Dynamic(d) => Some(ImplTarget::Object(d.trait_)),
        TyKind::RawPtr(m, _) => Some(ImplTarget::Primitive(flux_hir::raw_ptr_target(
            m == Mutability::Mut,
        ))),
        _ => None,
    }
}

/// The payload of the trusted alloc `Arc` or `Rc`, whose library
/// representations (a header address, then the data's) are shared with
/// array-owner and trait-object unsizing. User types with the same name do
/// not acquire this coercion, including in item trees without embedded
/// libraries.
pub fn shared_pointee(items: &ItemTree, ty: Ty) -> Option<Ty> {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    let shared = flux_solve::is_lang_struct(items, adt.strukt, "arc")
        || flux_solve::is_lang_struct(items, adt.strukt, "rc");
    (args.len() == 1 && shared).then(|| args[0])
}

/// The payloads of `from` and `to`, if both are the same one of `Arc` and
/// `Rc`, as [`shared_pointee`] reads them.
pub fn shared_pointees(items: &ItemTree, from: Ty, to: Ty) -> Option<(Ty, Ty)> {
    match (from.kind(), to.kind()) {
        (TyKind::Adt(a, _), TyKind::Adt(b, _)) if a.strukt == b.strukt => {
            shared_pointee(items, from).zip(shared_pointee(items, to))
        }
        _ => None,
    }
}

/// Pointees of matching trusted pointers wrapped in core Pin.
pub(crate) fn pinned_pointees(items: &ItemTree, from: Ty, to: Ty) -> Option<(Ty, Ty)> {
    let from = flux_solve::pin_pointer(items, from)?;
    let to = flux_solve::pin_pointer(items, to)?;
    match (from.kind(), to.kind()) {
        (TyKind::Ref(a, source), TyKind::Ref(b, target)) if a == b || a == Mutability::Mut => {
            Some((source, target))
        }
        _ => flux_solve::box_pointee(from)
            .zip(flux_solve::box_pointee(to))
            .or_else(|| shared_pointees(items, from, to)),
    }
}

/// The payload of core's trusted `NonNull`, whose pointer gains slice metadata
/// during an array coercion. Names in user crates confer no coercion capability.
pub fn nonnull_pointee(items: &ItemTree, ty: Ty) -> Option<Ty> {
    let TyKind::Adt(adt, args) = ty.kind() else {
        return None;
    };
    (args.len() == 1 && flux_solve::is_lang_struct(items, adt.strukt, "non_null")).then(|| args[0])
}

/// Integer representation of a trusted core nonzero wrapper.
pub fn nonzero_integer(items: &ItemTree, strukt: StructId) -> Option<IntTy> {
    static NAMES: std::sync::LazyLock<Vec<(Symbol, IntTy)>> = std::sync::LazyLock::new(|| {
        IntTy::ALL
            .into_iter()
            .map(|integer| {
                (
                    Symbol::intern(&format!("nonzero_{}", integer.name())),
                    integer,
                )
            })
            .collect()
    });
    NAMES.iter().find_map(|(name, integer)| {
        (items.lang_enums.get(name) == Some(&strukt)).then_some(*integer)
    })
}

/// What `Self` means in an item in `owner`, if any: an `impl` block's
/// type, or a trait's first parameter.
pub fn self_ty(sigs: &Signatures, owner: Option<Owner>) -> Option<Ty> {
    match owner? {
        Owner::Impl(imp) => Some(sigs.impls.get(imp).copied().unwrap_or(Ty::ERROR)),
        Owner::Trait(_) => Some(Ty::param(0, kw::SelfUpper)),
    }
}

pub(super) fn resolve_or_report(
    items: &ItemTree,
    module: ModuleId,
    ty: &TypeRef,
    scope: TypeScope<'_>,
    range: TextRange,
    diagnostics: &mut Vec<Diagnostic>,
) -> Ty {
    let mut errors = Vec::new();
    let ty = resolve_type(items, module, ty, scope, range, &mut errors);
    diagnostics.extend(
        errors
            .into_iter()
            .map(|(at, message)| Diagnostic::new(at, message)),
    );
    ty
}

/// Whether `ty` contains a reference. References are allowed only as
/// parameters and in local variables so far, where flux can check them
/// without lifetimes.
pub(crate) fn contains_ref(ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Ref(..) => true,
        TyKind::RawPtr(_, pointee) | TyKind::Array(pointee, _) | TyKind::Slice(pointee) => {
            contains_ref(pointee)
        }
        TyKind::Tuple(elements) | TyKind::Adt(_, elements) => {
            elements.iter().any(|&e| contains_ref(e))
        }
        _ => false,
    }
}
