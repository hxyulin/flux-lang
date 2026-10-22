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

mod infer;
mod pats;
pub mod traits;
mod ty;

use std::fmt::Write;
use std::ops::Index;
use std::sync::OnceLock;

use flux_arena::ArenaMap;
use flux_hir::def_map::{Def, Ns, ResolveError, Resolved};
use flux_hir::{
    Const, ConstId, Core, Diagnostic, Function, FunctionId, Generics, Hir, Impl, ImplId,
    ImplTarget, ItemCounts, ItemLoc, ItemTree, ModuleId, Owner, Path, Struct, StructId, Trait,
    TypeRef,
};
use flux_intern::{Symbol, kw, sym};
use flux_syntax::TextRange;

pub use infer::{InferenceResult, Method, c_char_ptr, c_str_ref, infer_body, infer_const};
pub use traits::{Predicate, Projection, TraitRef, TraitSig};
pub use ty::{
    AdtId, AliasTy, GenericArgs, InferVar, IntTy, Mutability, ParamTy, Ty, TyKind, subst_list,
};

/// A function's resolved signature. A generic function's mentions its
/// parameters, its `impl` block's first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
    pub params: Box<[Ty]>,
    pub ret: Ty,
}

/// A function with generic arguments for its parameters, its `impl`
/// block's first: what a call calls, and, once monomorphized, a function
/// in the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instance {
    pub function: FunctionId,
    pub args: GenericArgs,
}

/// A constant with generic arguments for its owner's parameters: a
/// trait's, `Self` first, or its `impl` block's. What a path to an
/// associated constant names; which value it is follows once the
/// arguments are known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConstInstance {
    pub konst: ConstId,
    pub args: GenericArgs,
}

impl FnSig {
    /// The signature of the instance with generic arguments `args`.
    pub fn subst(&self, args: &[Ty]) -> FnSig {
        FnSig {
            params: self.params.iter().map(|p| p.subst(args)).collect(),
            ret: self.ret.subst(args),
        }
    }
}

/// Everything about a file's items that bodies depend on: each
/// function's signature, each struct's field types, each `impl` block's
/// type and trait, and the bounds of each item, in terms of their
/// generic parameters.
#[derive(Debug, Clone, Default)]
pub struct Signatures {
    pub fns: ArenaMap<Function, FnSig>,
    /// The types of each struct's fields, and of each enum's, every
    /// variant's in turn: all that a value of it may contain.
    pub structs: ArenaMap<Struct, &'static [Ty]>,
    /// Where each variant of an enum starts in its `structs` entry, and
    /// where the last ends.
    pub variant_starts: ArenaMap<Struct, Box<[u32]>>,
    /// The type of each enum's discriminant: [`discriminant_ty`].
    pub discriminants: ArenaMap<Struct, IntTy>,
    pub impls: ArenaMap<Impl, Ty>,
    pub consts: ArenaMap<Const, Ty>,
    pub traits: ArenaMap<Trait, TraitSig>,
    /// The trait each trait's `impl` block implements, if it resolved.
    pub impl_traits: ArenaMap<Impl, TraitRef>,
    /// The bounds of each `impl` block, trait and function, their own
    /// only: a function's are after its owner's.
    pub impl_predicates: ArenaMap<Impl, Box<[Predicate]>>,
    /// The bounds of each struct, which every type naming it must meet.
    pub struct_predicates: ArenaMap<Struct, Box<[Predicate]>>,
    pub trait_predicates: ArenaMap<Trait, Box<[Predicate]>>,
    pub fn_predicates: ArenaMap<Function, Box<[Predicate]>>,
    /// What the bounds of each `impl` block, trait and function say
    /// associated types are (`T: Tr<Out = i32>`), their own only.
    pub impl_projections: ArenaMap<Impl, Box<[Projection]>>,
    pub struct_projections: ArenaMap<Struct, Box<[Projection]>>,
    pub trait_projections: ArenaMap<Trait, Box<[Projection]>>,
    pub fn_projections: ArenaMap<Function, Box<[Projection]>>,
    /// The associated types each trait's `impl` block defines, by name.
    pub impl_types: ArenaMap<Impl, Box<[(Symbol, Ty)]>>,
    /// The defaults of each trait's parameters.
    pub trait_defaults: traits::TraitDefaults,
    /// A trait is its own supertrait (reported): rustc stops before
    /// type checking bodies.
    pub cyclic: bool,
}

impl Index<FunctionId> for Signatures {
    type Output = FnSig;

    fn index(&self, function: FunctionId) -> &FnSig {
        &self.fns[function]
    }
}

impl Signatures {
    /// The types of the fields of `ty`, a tuple or struct; none for an
    /// enum, whose fields are its variants'.
    pub fn fields(&self, ty: Ty) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt, _) if self.is_enum(adt.strukt) => None,
            TyKind::Adt(adt, args) => Some(self.field_types(adt.strukt, args)),
            _ => None,
        }
    }

    /// The type of the discriminant of `ty`, an enum.
    pub fn discriminant_ty(&self, ty: Ty) -> Option<IntTy> {
        match ty.kind() {
            TyKind::Adt(adt, _) => self.discriminants.get(adt.strukt).copied(),
            _ => None,
        }
    }

    /// Whether `strukt` is an enum.
    pub fn is_enum(&self, strukt: StructId) -> bool {
        self.variant_starts.get(strukt).is_some()
    }

    /// The types of the fields of variant `variant` of `ty`, an enum, or
    /// of `ty` itself, a struct or tuple, for variant 0.
    pub fn variant_fields(&self, ty: Ty, variant: u32) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Adt(adt, args) if self.is_enum(adt.strukt) => {
                let starts = &self.variant_starts[adt.strukt];
                let (start, end) = (
                    *starts.get(variant as usize)? as usize,
                    *starts.get(variant as usize + 1)? as usize,
                );
                Some(&self.field_types(adt.strukt, args)[start..end])
            }
            _ => self.fields(ty),
        }
    }

    /// The types of everything a value of `ty` may hold directly: a
    /// tuple's elements, a struct's fields, every variant's fields of an
    /// enum.
    pub fn contents(&self, ty: Ty) -> Option<&'static [Ty]> {
        match ty.kind() {
            TyKind::Tuple(elements) => Some(elements),
            TyKind::Adt(adt, args) => Some(self.field_types(adt.strukt, args)),
            _ => None,
        }
    }

    /// The variants of `ty`, an enum, by their fields' types.
    pub fn variants(&self, ty: Ty) -> Option<Vec<&'static [Ty]>> {
        let TyKind::Adt(adt, _) = ty.kind() else {
            return None;
        };
        let count = self.variant_starts.get(adt.strukt)?.len() - 1;
        (0..count as u32)
            .map(|v| self.variant_fields(ty, v))
            .collect()
    }

    /// The types of the fields of `strukt` with generic arguments `args`,
    /// with the associated types they name (`T::Out`) normalized where
    /// `args` make them known.
    pub fn field_types(&self, strukt: StructId, args: &[Ty]) -> &'static [Ty] {
        let fields = self.structs[strukt];
        if args.is_empty() {
            return fields;
        }
        let fields = subst_list(fields, args);
        if !fields.iter().any(|f| f.has_aliases()) {
            return Ty::list(&fields);
        }
        let (mut done, mut active) = (rustc_hash::FxHashMap::default(), Vec::new());
        let fields: Vec<Ty> = fields
            .iter()
            .map(|&f| self.normalize_known(f, 0, &mut done, &mut active))
            .collect();
        Ty::list(&fields)
    }

    /// `ty` with each associated type of known types replaced by what
    /// their trait's `impl` block for them says it is. `done` has what
    /// the types met so far normalize to, and `active` the aliases being
    /// normalized, as [`traits::normalize`] does.
    fn normalize_known(
        &self,
        ty: Ty,
        depth: u32,
        done: &mut rustc_hash::FxHashMap<Ty, Ty>,
        active: &mut Vec<Ty>,
    ) -> Ty {
        if !ty.has_aliases() || depth > 64 || active.contains(&ty) {
            return ty;
        }
        if let Some(&done) = done.get(&ty) {
            return done;
        }
        active.push(ty);
        let mut go = |t: Ty| self.normalize_known(t, depth + 1, done, active);
        let normalized = match ty.kind() {
            TyKind::Alias(alias) => 'alias: {
                let args: Vec<Ty> = alias.args.iter().map(|&a| go(a)).collect();
                let alias = ty::AliasTy {
                    args: Ty::list(&args),
                    ..alias
                };
                for (imp, trait_ref) in self.impl_traits.iter() {
                    if trait_ref.trait_ != alias.trait_ {
                        continue;
                    }
                    // Every parameter of a trait's block is in its header.
                    let header =
                        std::iter::once(self.impls[imp]).chain(trait_ref.args.iter().copied());
                    let count = header.clone().map(max_param).max().unwrap_or(0);
                    let mut binds = vec![None; count as usize];
                    let matches = header
                        .zip(&args)
                        .all(|(x, &y)| traits::match_ty(x, y, &mut binds));
                    if matches && let Some(t) = traits::impl_type(self, imp, alias.name) {
                        let binds: Vec<Ty> =
                            binds.into_iter().map(|b| b.unwrap_or(Ty::ERROR)).collect();
                        break 'alias go(t.subst(&binds));
                    }
                }
                Ty::alias(alias)
            }
            TyKind::Tuple(list) => Ty::tuple(&list.iter().map(|&t| go(t)).collect::<Vec<_>>()),
            TyKind::Adt(adt, list) => {
                let list: Vec<Ty> = list.iter().map(|&t| go(t)).collect();
                Ty::adt(adt.strukt, adt.name, &list)
            }
            TyKind::Ref(m, t) => Ty::new(TyKind::Ref(m, go(t))),
            TyKind::RawPtr(m, t) => Ty::raw_ptr(m, go(t)),
            _ => ty,
        };
        active.pop();
        done.insert(ty, normalized);
        normalized
    }
}

/// One more than the largest index of a generic parameter in `ty`, or 0.
fn max_param(ty: Ty) -> u32 {
    match ty.kind() {
        TyKind::Param(p) => p.index + 1,
        TyKind::Tuple(list) | TyKind::Adt(_, list) => {
            list.iter().map(|&t| max_param(t)).max().unwrap_or(0)
        }
        TyKind::Alias(alias) => alias.args.iter().map(|&t| max_param(t)).max().unwrap_or(0),
        TyKind::Ref(_, t) | TyKind::RawPtr(_, t) => max_param(t),
        _ => 0,
    }
}

/// What the names in a type mean besides items: `Self`, if anything,
/// and the generic parameters in scope, by index.
#[derive(Debug, Clone, Copy, Default)]
pub struct TypeScope<'a> {
    pub self_ty: Option<Ty>,
    pub generics: &'a [Symbol],
    /// In an item's signature, where `_` is not allowed, what rustc
    /// calls such items ("functions"); `None` in a body, where `_` is a
    /// type to infer.
    pub signature_of: Option<&'static str>,
    /// The bounds in scope, with what they imply: where `T::Out` finds
    /// its trait.
    pub bounds: &'a [Predicate],
    /// In a trait's `impl` block, its trait: `Self::Out` is its type.
    pub self_trait: Option<TraitRef>,
    /// The defaults of traits' parameters, for bounds that leave them
    /// out.
    pub defaults: Option<&'a traits::TraitDefaults>,
}

impl TypeScope<'_> {
    /// The generic parameter called `name`, if one is in scope.
    pub fn param(&self, name: Symbol) -> Option<Ty> {
        let index = self.generics.iter().rposition(|&n| n == name)?;
        Some(Ty::param(index as u32, name))
    }
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
                Ok(Resolved::Primitive(name)) => {
                    no_args(errors, format!("builtin type `{name}`"));
                    match primitive(name) {
                        Ok(ty) => ty,
                        Err(message) => fail(errors, range, message),
                    }
                }
                // A bare trait is a trait object in edition 2015 only.
                Ok(Resolved::Def(Def::Trait(_))) => fail(
                    errors,
                    last_range,
                    "expected a type, found a trait".to_owned(),
                ),
                Ok(Resolved::Def(Def::Module(m))) => fail(
                    errors,
                    last_range,
                    format!("expected type, found module `{}`", items.modules[m].name),
                ),
                Ok(Resolved::Def(Def::Variant(..))) => fail(
                    errors,
                    range,
                    format!("expected type, found variant `{path}`"),
                ),
                Ok(_) => fail(errors, last_range, format!("expected type, found `{path}`")),
                Err(e @ ResolveError::NotFound { .. }) => {
                    let ResolveError::NotFound { segment, module } = e else {
                        unreachable!()
                    };
                    let name = path.segments[segment];
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
            Ty::tuple(&fields)
        }
        TypeRef::Never => Ty::NEVER,
        TypeRef::Ptr { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::raw_ptr(mutability(*mutable), pointee)
        }
        TypeRef::Ref { mutable, pointee } => {
            let pointee = resolve_type(items, module, pointee, scope, range, errors);
            if pointee == Ty::ERROR {
                return Ty::ERROR;
            }
            Ty::new(TyKind::Ref(mutability(*mutable), pointee))
        }
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
    if let Some(args) = path.args(name_index) {
        let at = args.ranges.first().copied().unwrap_or(args.range);
        errors.push((at, "generic associated types are not supported yet".into()));
        return Ty::ERROR;
    }
    let has_type = |t: flux_hir::TraitId| items.traits[t].types.iter().any(|ty| ty.name == name);
    // `<T as Tr>::Out`: the trait is written.
    if let Some((trait_path, qrange)) = trait_path {
        let bound = flux_hir::Bound {
            path: trait_path.clone(),
            range: qrange,
        };
        let Some((trait_ref, _)) =
            traits::resolve_bound(items, module, &bound, self_ty, scope, errors)
        else {
            return Ty::ERROR;
        };
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
        return Ty::alias(traits::alias_of(trait_ref, self_ty, name));
    }
    // `Self::Out` in a trait's `impl` block: the block's own.
    if let Some(trait_ref) = scope.self_trait
        && Some(self_ty) == scope.self_ty
    {
        if has_type(trait_ref.trait_) {
            return Ty::alias(traits::alias_of(trait_ref, self_ty, name));
        }
        if path.qself.is_none() && path.segments[0] == kw::SelfUpper {
            errors.push((at, format!("associated type `{name}` not found for `Self`")));
            return Ty::ERROR;
        }
    }
    if !matches!(self_ty.kind(), TyKind::Param(_)) {
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
        [trait_ref] => Ty::alias(traits::alias_of(*trait_ref, self_ty, name)),
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
fn resolve_predicates(
    items: &ItemTree,
    sigs: &Signatures,
    module: ModuleId,
    generics: &Generics,
    scope: TypeScope<'_>,
    errors: &mut Vec<(TextRange, String)>,
) -> traits::Bounds {
    let mut first = Vec::new();
    let resolved = traits::resolve_predicates(items, module, generics, scope, &mut first);
    if first.is_empty() {
        return resolved;
    }
    let all: Vec<Predicate> = scope
        .bounds
        .iter()
        .chain(&resolved.predicates)
        .copied()
        .collect();
    let bounds = traits::elaborate(sigs, &all);
    let scope = TypeScope {
        bounds: &bounds,
        ..scope
    };
    traits::resolve_predicates(items, module, generics, scope, errors)
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
    let last = path.segments.len().saturating_sub(1);
    let at = path.range(last, range);
    let want = def.generics.len();
    for args in path.args.iter().flatten() {
        for b in args.bindings.iter() {
            errors.push((b.range(), flux_hir::MISPLACED_BINDING.into()));
        }
    }
    let Some(written) = path.args(last) else {
        if want == 0 {
            return Ty::adt(strukt, def.name, &[]);
        }
        let kind = if def.is_enum() { "enum" } else { "struct" };
        errors.push((at, format!("missing generics for {kind} `{}`", def.name)));
        return Ty::ERROR;
    };
    let args: Vec<Ty> = written
        .types
        .iter()
        .zip(&written.ranges)
        .map(|(ty, &range)| resolve_type(items, module, ty, scope, range, errors))
        .collect();
    if args.len() != want {
        let kind = if def.is_enum() { "enum" } else { "struct" };
        errors.push((at, wrong_arg_count(kind, want, args.len())));
        return Ty::ERROR;
    }
    if args.contains(&Ty::ERROR) {
        return Ty::ERROR;
    }
    Ty::adt(strukt, def.name, &args)
}

/// The primitive type called `name`.
pub fn primitive(name: Symbol) -> Result<Ty, String> {
    if name == sym::bool {
        return Ok(Ty::BOOL);
    }
    match IntTy::from_name(name) {
        Some(int) => Ok(Ty::int(int)),
        None => Err(format!("`{name}` is not supported yet")),
    }
}

/// The `impl` blocks' type for methods on `ty`, if it has any.
pub fn impl_target(ty: Ty) -> Option<ImplTarget> {
    match ty.kind() {
        TyKind::Adt(adt, _) => Some(ImplTarget::Struct(adt.strukt)),
        TyKind::Int(int) => Some(ImplTarget::Primitive(int.name())),
        TyKind::Bool => Some(ImplTarget::Primitive(sym::bool)),
        _ => None,
    }
}

/// What `Self` means in an item in `owner`, if any: an `impl` block's
/// type, or a trait's first parameter.
pub fn self_ty(sigs: &Signatures, owner: Option<Owner>) -> Option<Ty> {
    match owner? {
        Owner::Impl(imp) => Some(sigs.impls.get(imp).copied().unwrap_or(Ty::ERROR)),
        Owner::Trait(_) => Some(Ty::param(0, kw::SelfUpper)),
    }
}

fn resolve_or_report(
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
        TyKind::RawPtr(_, pointee) => contains_ref(pointee),
        TyKind::Tuple(elements) | TyKind::Adt(_, elements) => {
            elements.iter().any(|&e| contains_ref(e))
        }
        _ => false,
    }
}

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

/// Whether two `impl` blocks' types, each in terms of its own
/// parameters, may be the same type.
fn overlap(a: Ty, b: Ty) -> bool {
    match (a.kind(), b.kind()) {
        (TyKind::Param(_) | TyKind::Error, _) | (_, TyKind::Param(_) | TyKind::Error) => true,
        (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) => {
            x == y && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Tuple(xs), TyKind::Tuple(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys).all(|(&a, &b)| overlap(a, b))
        }
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && overlap(x, y)
        }
        _ => a == b,
    }
}

/// Whether `ty` mentions generic parameter `index`.
fn mentions_param(ty: Ty, index: u32) -> bool {
    if !ty.has_params() {
        return false;
    }
    match ty.kind() {
        TyKind::Param(param) => param.index == index,
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) => mentions_param(t, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) => {
            list.iter().any(|&t| mentions_param(t, index))
        }
        _ => false,
    }
}

/// Whether `ty` uses generic parameter `index`, through associated types
/// (`T::Out`) too, as a struct's fields may.
fn uses_param(ty: Ty, index: u32) -> bool {
    match ty.kind() {
        TyKind::Alias(alias) => alias.args.iter().any(|&t| uses_param(t, index)),
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) => uses_param(t, index),
        TyKind::Tuple(list) | TyKind::Adt(_, list) => list.iter().any(|&t| uses_param(t, index)),
        _ => mentions_param(ty, index),
    }
}

/// The parameters of `generics` as types, numbered from `first`.
pub fn params_of(generics: &Generics, first: usize) -> Vec<Ty> {
    generics
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| Ty::param((first + i) as u32, p.name))
        .collect()
}

/// The signature of every function and the field types of every struct
/// in `items`.
pub fn signatures(items: &ItemTree) -> (Signatures, Vec<Diagnostic>) {
    signatures_checked(items, false)
}

/// Like [`signatures`]; `check_core` checks `core`'s `impl` blocks
/// against their traits too, which its tests do instead of every build.
///
/// `core`'s signatures are resolved once per process; a program's are
/// resolved after them, unless `check_core` resolves everything again.
pub fn signatures_checked(items: &ItemTree, check_core: bool) -> (Signatures, Vec<Diagnostic>) {
    let core = items.core;
    if check_core || core == ItemCounts::default() {
        let checked = if check_core { 0 } else { core.impls };
        return extend_signatures(items, Signatures::default(), ItemCounts::default(), checked);
    }
    let (sigs, mut diagnostics) = core_signatures().clone();
    let (sigs, more) = extend_signatures(items, sigs, core, core.impls);
    diagnostics.extend(more);
    (sigs, diagnostics)
}

/// The signatures of [`Core`]'s items, its `impl` blocks unchecked as in
/// every build, resolved the first time they are asked for.
fn core_signatures() -> &'static (Signatures, Vec<Diagnostic>) {
    static CORE: OnceLock<(Signatures, Vec<Diagnostic>)> = OnceLock::new();
    CORE.get_or_init(|| {
        let items = &Core::get().items;
        let unchecked = items.impls.len() as u32;
        extend_signatures(
            items,
            Signatures::default(),
            ItemCounts::default(),
            unchecked,
        )
    })
}

/// Resolves the signatures of `items` from `from` on, given `sigs`, the
/// signatures of those before, which do not depend on them, and checks
/// them; the `impl` blocks from `checked_impls` on are checked against
/// their traits.
fn extend_signatures(
    items: &ItemTree,
    mut sigs: Signatures,
    from: ItemCounts,
    checked_impls: u32,
) -> (Signatures, Vec<Diagnostic>) {
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
        let ty = impl_type(items, imp, &mut reported);
        sigs.impls.insert(id, ty);
    }
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
        let scope = TypeScope {
            self_ty: Some(Ty::param(0, kw::SelfUpper)),
            generics: &generics,
            signature_of: Some("traits"),
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let resolved: Box<[Option<Ty>]> = t
            .generics
            .params
            .iter()
            .map(|p| {
                let (ty, range) = p.default.as_ref()?;
                Some(resolve_type(
                    items,
                    t.loc.module,
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
        let self_param = Ty::param(0, kw::SelfUpper);
        let bounds = [traits::trait_self_predicate(items, id)];
        let scope = TypeScope {
            self_ty: Some(self_param),
            generics: &generics,
            signature_of: Some("traits"),
            bounds: &bounds,
            defaults: Some(&sigs.trait_defaults),
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
        let generics: Vec<Symbol> = s.generics.params.iter().map(|p| p.name).collect();
        let scope = TypeScope {
            self_ty: Some(Ty::adt(id, s.name, &params_of(&s.generics, 0))),
            generics: &generics,
            signature_of: Some("structs"),
            defaults: Some(&sigs.trait_defaults),
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let own = resolve_predicates(items, &sigs, s.loc.module, &s.generics, scope, &mut errors);
        in_file(errors, s.loc.file, &mut reported);
        let bounds = traits::elaborate(&sigs, &own.predicates);
        let scope = TypeScope {
            bounds: &bounds,
            ..scope
        };
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
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        if imp.trait_ref.is_none() && imp.generics.predicates.is_empty() {
            continue;
        }
        let generics = generic_names(items, Some(Owner::Impl(id)), &Generics::default());
        let self_ty = sigs.impls[id];
        let scope = TypeScope {
            self_ty: Some(self_ty),
            generics: &generics,
            signature_of: Some("implementations"),
            defaults: Some(&sigs.trait_defaults),
            ..TypeScope::default()
        };
        let mut errors = Vec::new();
        let mut self_trait = None;
        let mut projections = Vec::new();
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
        let resolved =
            resolve_predicates(items, &sigs, imp.module, &imp.generics, scope, &mut errors);
        projections.extend(resolved.projections);
        let bounds = traits::elaborate(&sigs, &resolved.predicates);
        let scope = TypeScope {
            bounds: &bounds,
            self_trait,
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
        let generics = generic_names(items, konst.owner, &Generics::default());
        // Its owner's bounds, for `Self::A` or `T::A` in its type.
        let bounds = traits::elaborate(&sigs, &traits::owner_predicates(items, &sigs, konst.owner));
        let scope = TypeScope {
            self_ty: self_ty(&sigs, konst.owner),
            generics: &generics,
            signature_of: Some(match konst.owner {
                Some(_) => "associated constants",
                None => "constants",
            }),
            bounds: &bounds,
            self_trait: konst
                .owner
                .and_then(Owner::as_impl)
                .and_then(|imp| sigs.impl_traits.get(imp).copied()),
            defaults: Some(&sigs.trait_defaults),
        };
        let ty = resolve(
            items,
            konst.loc,
            &konst.ty,
            scope,
            konst.ty_range,
            &mut reported,
        );
        sigs.consts.insert(id, ty);
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let generics = generic_names(items, function.owner, &function.generics);
        let self_trait = items
            .impl_of(id)
            .and_then(|imp| sigs.impl_traits.get(imp).copied());
        let owner_bounds = traits::owner_predicates(items, &sigs, function.owner);
        let scope = TypeScope {
            self_ty: self_ty(&sigs, function.owner),
            generics: &generics,
            signature_of: Some("functions"),
            self_trait,
            defaults: Some(&sigs.trait_defaults),
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
        let params = function
            .params
            .iter()
            .map(|p| resolve(items, function.loc, &p.ty, scope, p.range, &mut reported))
            .collect();
        let ret = resolve(
            items,
            function.loc,
            &function.ret,
            scope,
            function.ret_range,
            &mut reported,
        );
        sigs.fns.insert(id, FnSig { params, ret });
        if !own.predicates.is_empty() {
            sigs.fn_predicates.insert(id, own.predicates.into());
        }
        if !own.projections.is_empty() {
            sigs.fn_projections.insert(id, own.projections.into());
        }
    }
    for (id, s) in items.structs.iter().skip(from.structs as usize) {
        // Without `PhantomData`, a parameter must be a field's type, of
        // some variant for an enum.
        for (i, param) in s.generics.params.iter().enumerate() {
            let fields = sigs.structs[id];
            if !fields.iter().any(|&f| uses_param(f, i as u32)) && !fields.contains(&Ty::ERROR) {
                error(
                    s.loc.file,
                    param.range,
                    &format!("type parameter `{}` is never used", param.name),
                );
            }
        }
    }
    traits::check_well_formed(items, &sigs, from, &mut reported);
    duplicate_assoc_items(items, &sigs, &mut reported, from.impls as usize);
    traits::check_impls(items, &sigs, &mut reported, checked_impls);
    sigs.cyclic |= traits::check_supertrait_cycles(items, &sigs, &mut reported, from);
    traits::check_trivial_bounds(items, &sigs, &mut reported, from);
    for (id, imp) in items.impls.iter().skip(from.impls as usize) {
        let ty = sigs.impls[id];
        let trait_args = sigs.impl_traits.get(id).map_or(&[][..], |t| t.args);
        for (i, param) in imp.generics.params.iter().enumerate() {
            let constrained = mentions_param(ty, i as u32)
                || trait_args.iter().any(|&a| mentions_param(a, i as u32));
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
        for (field, &ty) in all_fields(s).zip(sigs.structs[id]) {
            // With lifetime parameters, already reported.
            if contains_ref(ty) && !s.generics.has_lifetimes {
                error(s.loc.file, field.range, "missing lifetime specifier");
            }
        }
    }
    for (id, function) in items.functions.iter().skip(from.functions as usize) {
        let sig = &sigs.fns[id];
        let params = function.params.iter().zip(&sig.params);
        if function.is_extern {
            for (param, &ty) in params.clone() {
                if contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "passing references to C is not supported yet",
                    );
                }
            }
            let types = params
                .map(|(p, &ty)| (ty, p.range))
                .chain([(sig.ret, function.ret_range)]);
            for (ty, range) in types {
                if !matches!(ty.kind(), TyKind::Tuple([])) && sigs.contents(ty).is_some() {
                    error(
                        function.loc.file,
                        range,
                        "passing structs and tuples to C is not supported yet",
                    );
                }
            }
        } else {
            for (param, &ty) in params {
                if !matches!(ty.kind(), TyKind::Ref(..)) && contains_ref(ty) {
                    error(
                        function.loc.file,
                        param.range,
                        "references inside other types are not supported yet",
                    );
                }
            }
        }
        if contains_ref(sig.ret) {
            if sig.params.iter().any(|&p| contains_ref(p)) {
                error(
                    function.loc.file,
                    function.ret_range,
                    "returning references is not supported yet",
                );
            } else {
                error(
                    function.loc.file,
                    function.ret_range,
                    "missing lifetime specifier",
                );
            }
        }
    }
    diagnostics.extend(reported);
    (sigs, diagnostics)
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
    let fields = sigs.contents(ty).unwrap_or(&[]);
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
fn impl_type(items: &ItemTree, imp: &Impl, diagnostics: &mut Vec<Diagnostic>) -> Ty {
    let generics: Vec<Symbol> = imp.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        self_ty: None,
        generics: &generics,
        signature_of: Some("implementations"),
        ..TypeScope::default()
    };
    let mut errors = Vec::new();
    if imp.trait_ref.is_some() {
        let ty = resolve_or_report(
            items,
            imp.module,
            &imp.self_ty,
            scope,
            imp.self_range,
            diagnostics,
        );
        for d in diagnostics
            .iter_mut()
            .filter(|d| d.file == flux_hir::FileId::UNKNOWN)
        {
            d.file = imp.file;
        }
        return ty;
    }
    let ty = match (imp.target, &imp.self_ty) {
        (Some(ImplTarget::Struct(strukt)), TypeRef::Path(path)) => struct_type(
            items,
            imp.module,
            strukt,
            path,
            scope,
            imp.self_range,
            &mut errors,
        ),
        (Some(ImplTarget::Primitive(name)), _) => primitive(name).unwrap_or(Ty::ERROR),
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
    let main = items.functions.iter().find(|(_, f)| {
        f.name == sym::main && !f.is_extern && f.owner.is_none() && f.loc.module == root
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
    } else if function.is_unsafe {
        "`main` function is not allowed to be `unsafe`".to_owned()
    } else if !sig.params.is_empty() {
        "`main` function cannot take parameters".to_owned()
    } else if ![Ty::UNIT, Ty::NEVER, Ty::ERROR].contains(&sig.ret) {
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
    }) || items
        .functions
        .values()
        .any(|f| in_generics(f.loc.module, &f.generics))
}

/// Infers every body of the program, and those of `core` it may call.
pub fn check_file(hir: &Hir) -> TypedFile {
    check(hir, false)
}

/// Like [`check_file`], with every body of `core` too: for tests of
/// `core` itself.
pub fn check_file_all(hir: &Hir) -> TypedFile {
    check(hir, true)
}

/// The functions a call of `function` may run: itself, and for a
/// trait's method, its definition in each `impl` block of the trait.
pub fn call_targets(items: &ItemTree, function: FunctionId) -> Vec<FunctionId> {
    let mut out = vec![function];
    if let Some(Owner::Trait(t)) = items.functions[function].owner {
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

fn check(hir: &Hir, all: bool) -> TypedFile {
    let (sigs, mut diagnostics) = signatures_checked(&hir.items, all);
    let mut bodies = ArenaMap::new();
    if sigs.cyclic || unresolved_trait(&hir.items) {
        Diagnostic::sort(&mut diagnostics);
        return TypedFile {
            sigs,
            bodies,
            consts: ArenaMap::new(),
            diagnostics,
        };
    }
    let items = &hir.items;
    let mut queue: Vec<FunctionId> = Vec::new();
    let reached = |result: &InferenceResult, queue: &mut Vec<FunctionId>| {
        let called = result.calls.iter().map(|(_, i)| i.function);
        let methods = result.methods.iter().map(|(_, m)| m.function);
        let operators = result.overloads.iter().map(|(_, i)| i.function);
        for f in called.chain(methods).chain(operators) {
            queue.extend(call_targets(items, f));
        }
    };
    let mut consts = ArenaMap::new();
    for (id, (body, source_map)) in hir.const_bodies() {
        let result = infer_const(items, &sigs, id, body, source_map);
        let file = items.consts[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
        reached(&result, &mut queue);
        consts.insert(id, result);
    }
    // The program's bodies, then the `core` bodies they reach.
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
    while let Some(id) = queue.pop() {
        if bodies.get(id).is_some() {
            continue;
        }
        let Some((body, source_map)) = hir.body(id) else {
            continue;
        };
        let result = infer_body(items, &sigs, id, body, source_map);
        let file = items.functions[id].loc.file;
        diagnostics.extend(
            result
                .diagnostics
                .iter()
                .map(|d| Diagnostic { file, ..d.clone() }),
        );
        reached(&result, &mut queue);
        bodies.insert(id, result);
    }
    Diagnostic::sort(&mut diagnostics);
    TypedFile {
        sigs,
        bodies,
        consts,
        diagnostics,
    }
}

/// The checks of the patterns of the bodies whose patterns name
/// constants ([`InferenceResult::deferred_pats`]), given `value`, the
/// value of a constant of an integer type or `bool` (`1` for `true`),
/// or `None` if it has none (reported). Bodies that name one without a
/// value are not checked. Also returns the functions with errors, which
/// rustc does not borrow-check.
pub fn check_deferred_patterns(
    hir: &Hir,
    typed: &TypedFile,
    value: &mut dyn FnMut(ConstInstance) -> Option<i128>,
) -> (Vec<Diagnostic>, Vec<FunctionId>) {
    let functions = typed.bodies.iter().map(|(id, result)| {
        let file = hir.items.functions[id].loc.file;
        (Some(id), hir.body(id), file, result)
    });
    let consts = typed.consts.iter().map(|(id, result)| {
        let file = hir.items.consts[id].loc.file;
        (None, hir.const_body(id), file, result)
    });
    let mut diagnostics = Vec::new();
    let mut failed = Vec::new();
    'bodies: for (function, body, file, result) in functions.chain(consts) {
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
            if !hir.is_program(id) {
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
