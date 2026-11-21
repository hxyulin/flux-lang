//! Lifetimes in signatures, for the borrow checker.
//!
//! Types are checked without lifetimes: a [`Ty`] has none, and bodies
//! are inferred as if every reference had the same one, as rustc's type
//! checking erases them. What the borrow checker needs of lifetimes is
//! here, beside the types of signatures: the lifetime of each *region
//! slot* of each type in each function's, struct's and `impl` block's
//! signature. A type's slots are, in order: a reference's own, then its
//! pointee's; a struct's or enum's lifetime arguments, then its type
//! arguments' slots; a tuple's, array's or raw pointer's parts'; a
//! closure's captures'; and a trait object's lifetime bound (`dyn Tr +
//! 'a`, or rustc's default: the reference it is behind, `'static`
//! elsewhere). A generic parameter, an associated type, and a
//! function item or pointer have none: an instance of a signature has
//! the slots of its types with the arguments substituted, each
//! parameter's at its place ([`instantiate`]).
//!
//! A signature's lifetimes are its parameters (its owner's, then its
//! own, then one for each lifetime elided in a function's inputs or an
//! `impl` block's header), `'static`, or, in a body, elided ones the
//! borrow checker infers. Elision follows rustc: in a function's return
//! type, an elided lifetime is that of `&self` or `&mut self`, or the
//! only one in the inputs (E0106 otherwise); in a constant's or
//! static's type it is `'static`.

use flux_hir::def_map::{Def, Ns, Resolved};
use flux_hir::{
    GenericArgs, Generics, ItemTree, LifetimeRef, ModuleId, Path, TypeAlias, TypeAliasId, TypeRef,
};
use flux_intern::{Symbol, kw};
use flux_syntax::TextRange;

use crate::{ClosureArgs, Signatures, Ty, TyKind, TypeScope};

pub use flux_ty_ir::fn_regions::{
    instantiate as instantiate_fn_pointer, instantiate_with as instantiate_fn_pointer_with,
};
pub use flux_ty_ir::regions::{
    AliasRegions, CallbackBinder, FUNCTION_LIFETIMES_ARG, FamilyProjection, FamilyRegions,
    FnRegions, IMPL_SELF_ARG, ImplRegions, LifetimeName, OpaqueBoundRegions, OpaqueRegions,
    Outlives, ProjectionOutlives, ProjectionSelection, Region, RegionSigs, StructRegions,
    TRAIT_LIFETIMES_ARG, TraitRefRegions, TraitRegions, TypeArgRegions, Variance,
    function_item_owner_lifetimes, has_declared_projection_inputs, has_projection_inputs,
    instantiate, object_associated_range, object_own_slots, object_region_types, slots,
};

pub use crate::fn_regions::variances as fn_pointer_variances;

/// A concrete nominal type with no free lifetime or callable capture inputs.
pub fn is_closed_nominal(items: &ItemTree, ty: Ty) -> bool {
    fn has_callable_capture(ty: Ty) -> bool {
        matches!(ty.kind(), TyKind::Closure(..) | TyKind::FnDef(_))
            || ty.children().any(has_callable_capture)
    }

    matches!(ty.kind(), TyKind::Adt(..))
        && !ty.has_infer()
        && is_closed_region_free(items, ty)
        && !has_callable_capture(ty)
}

/// A concrete definition whose shape cannot acquire regions by substitution.
pub(crate) fn is_closed_region_free(items: &ItemTree, ty: Ty) -> bool {
    !ty.has_error()
        && !ty.has_params()
        && !ty.has_aliases()
        && !ty.has_opaques()
        && slots(items, ty) == 0
        && ty
            .children()
            .all(|child| is_closed_region_free(items, child))
}

pub(crate) const BODY_PROJECTION_LIFETIMES: &str =
    "lifetimes in associated type annotations are not supported yet";

mod method_binders;
mod object;
mod projection_outlives;
pub use projection_outlives::{signature_projection_outlives, type_projection_outlives};
mod resolve;
mod selected;
pub(crate) use selected::closed_impl_predicates_hold;
mod selected_signatures;
pub use method_binders::method_early_lifetimes;
pub use selected_signatures::{SelectedMethodSignature, selected_method_signature};
mod trait_profile;

pub use trait_profile::{
    TraitProfileError, TraitProfileReason, required_method_trait_profile, safe_method_trait_profile,
};

pub use resolve::{
    BoundProjectionSelections, TraitBoundRegions, function_bound_projection_selections,
    trait_bound_regions, trait_bound_schemes,
};
pub(crate) use resolve::{outlives_known, resolve_regions};

fn has_projection_outlives_inputs(items: &ItemTree, ty: Ty) -> bool {
    matches!(ty.kind(), TyKind::Alias(alias) if has_projection_inputs(items, alias))
        || ty
            .children()
            .any(|ty| has_projection_outlives_inputs(items, ty))
}

/// What an elided lifetime (`&T`, `'_`, or a struct's lifetime
/// arguments left out) is where a type is written.
#[derive(Debug, Clone)]
pub(crate) enum Elided {
    /// A new lifetime parameter: in a function's inputs and an `impl`
    /// block's header.
    Fresh,
    /// This one: a function's return type's, a constant's (`'static`),
    /// or one to infer in a body.
    Is(Region),
    /// Missing (E0106), with rustc's help.
    Missing(Option<String>),
    /// Not allowed (E0637): in a bound's generic arguments.
    NotAllowed,
}

/// Static Self bounds also determine an omitted object lifetime. This query
/// uses declarations because signature walkers run while Signatures is built.
/// Bounds on other type parameters never determine the object's lifetime.
pub(crate) fn object_has_static_self(items: &ItemTree, principal: flux_hir::TraitId) -> bool {
    let mut pending = vec![principal];
    let mut seen = rustc_hash::FxHashSet::default();
    while let Some(owner) = pending.pop() {
        if !seen.insert(owner) {
            continue;
        }
        let data = &items.traits[owner];
        let is_self =
            |ty: &TypeRef| matches!(ty,TypeRef::Path(path) if path.as_name()==Some(kw::SelfUpper));
        if data
            .generics
            .type_outlives
            .iter()
            .any(|bound| is_self(&bound.ty) && bound.lifetime.is_static())
        {
            return true;
        }
        for bound in data.supertraits.iter().chain(
            data.generics
                .predicates
                .iter()
                .filter(|predicate| is_self(&predicate.ty))
                .map(|predicate| &predicate.bound),
        ) {
            if let (Some(parent), _) = items.resolve_trait(data.loc.module, bound) {
                pending.push(parent);
            }
        }
    }
    false
}

/// Finds the regions of types as written, beside their resolved types.
#[derive(Clone)]
pub(crate) struct Walker<'a> {
    pub items: &'a ItemTree,
    pub module: ModuleId,
    /// The lifetime parameters in scope, by index; `None` for one an
    /// elided lifetime stands for, which cannot be named.
    pub names: Vec<Option<Symbol>>,
    /// The current Fn bound's names and their first input-slot positions.
    pub late_names: Vec<(Symbol, u32)>,
    /// What `Self` is, if it has slots.
    pub self_regions: Option<&'a [Region]>,
    /// In a trait's `impl` block's methods, the slots of its associated
    /// types, which `Self::Out` resolves to.
    pub self_types: &'a [(Symbol, Box<[Region]>)],
    /// Projection inputs from those same associated type definitions.
    pub self_type_selections: &'a [(Symbol, Box<[ProjectionSelection]>)],
    /// Inherited projections whose selected impl needs an unrepresented map.
    pub unmapped_self_types: &'a [Symbol],
    pub inherited_self_types: &'a [(Symbol, Box<[Region]>)],
    /// For resolving the types an alias is given.
    pub scope: TypeScope<'a>,
    /// Completed impl signatures for concrete qualified projection occurrences.
    pub signatures: Option<&'a Signatures>,
    /// Original caller equalities, before concrete impl selection.
    pub caller_projections: &'a [crate::Projection],
    /// A qualified body root retains the original inference variables until
    /// they are solved. Consume it before walking nested projection inputs.
    pub inferred_root_alias: Option<crate::AliasTy>,
    /// Selection inputs stay invariant even when normalization erases them.
    pub selected_inputs: Vec<(crate::AliasTy, Box<[Region]>)>,
    /// Use expression and local type rules, including inference for elided rows.
    pub body: bool,
    /// Omitted object bounds in expression type spellings are inferred.
    /// Alias declarations and generic defaults keep their declaration defaults.
    pub infer_objects: bool,
    pub elided: Elided,
    /// Trait-object lifetime arguments must be explicit in impl headers.
    pub in_impl_header: bool,
    /// The lifetime parameters elided lifetimes made, after `names`.
    pub fresh: Vec<TextRange>,
    /// In a function's return type, the lifetimes each of its opaque
    /// types captures, which are their slots.
    pub opaque_captures: Vec<Box<[Region]>>,
    pub errors: Vec<(TextRange, String)>,
    /// Enabled only while collecting an impl associated value's regions.
    pub selections: Option<Vec<ProjectionSelection>>,
    /// Method comparison captures only its supported qualified legacy inputs;
    /// declaration/header collectors keep all selection occurrences.
    pub qualified_legacy_selections: bool,
    /// Written-bound proofs also collect inputs hidden behind type aliases.
    pub expand_alias_selections: bool,
    /// Original trait bounds provide owner lifetime arguments for `T::Family`.
    projection_owners: Vec<ProjectionOwner>,
    /// An impl/trait's own trait arguments have already been resolved in its header.
    pub self_projection_owner: Option<(flux_hir::TraitId, Box<[Region]>)>,
    /// The region of the reference whose pointee is being walked, if
    /// that is what is: a trait object's lifetime bound there.
    ref_region: Option<Region>,
    inherited_projection_stack: Vec<crate::AliasTy>,
    /// A private candidate-counting view; never published as a signature row.
    elision_only: bool,
    /// In a body's written annotation, the owner lifetimes of a projection
    /// whose bound is not known here (`A::Error` for `A: MapAccess<'de>`)
    /// are left to inference, as an unwritten lifetime is.
    pub infer_unknown_owners: bool,
}

#[derive(Clone)]
struct ProjectionOwner {
    subject: Ty,
    trait_: flux_hir::TraitId,
    module: ModuleId,
    bound: flux_hir::Bound,
}

impl<'a> Walker<'a> {
    pub fn new(items: &'a ItemTree, module: ModuleId, names: Vec<Option<Symbol>>) -> Self {
        Walker {
            items,
            module,
            names,
            late_names: Vec::new(),
            self_regions: None,
            self_types: &[],
            self_type_selections: &[],
            unmapped_self_types: &[],
            inherited_self_types: &[],
            scope: TypeScope::default(),
            signatures: None,
            caller_projections: &[],
            inferred_root_alias: None,
            selected_inputs: Vec::new(),
            body: false,
            infer_objects: false,
            elided: Elided::Is(Region::Erased),
            in_impl_header: false,
            fresh: Vec::new(),
            opaque_captures: Vec::new(),
            errors: Vec::new(),
            selections: None,
            qualified_legacy_selections: false,
            expand_alias_selections: false,
            projection_owners: Vec::new(),
            self_projection_owner: None,
            ref_region: None,
            inherited_projection_stack: Vec::new(),
            elision_only: false,
            infer_unknown_owners: false,
        }
    }

    /// The slots of `ty`, written as `written`.
    pub fn regions(&mut self, written: &TypeRef, ty: Ty) -> Box<[Region]> {
        let mut out = Vec::new();
        self.walk(written, ty, &mut out);
        // A type that did not resolve as written (an error, reported)
        // has its slots inferred.
        let want = slots(self.items, ty);
        out.resize(want, Region::Erased);
        out.into()
    }

    /// Omitted object defaults do not contribute to reference-return elision.
    /// Reuse the full walker so aliases and repeated substitutions retain the
    /// same provenance, while actual well-formedness rows remain untouched.
    pub(crate) fn elision_regions(&self, written: &TypeRef, ty: Ty) -> Box<[Region]> {
        let mut view = self.clone();
        view.elision_only = true;
        view.regions(written, ty)
            .iter()
            .copied()
            .filter(|r| *r != Region::Erased)
            .collect()
    }

    /// The region a lifetime written as `lifetime` is.
    pub fn named(&mut self, lifetime: LifetimeRef) -> Region {
        if lifetime.is_static() {
            return Region::Static;
        }
        if lifetime.is_anonymous() {
            return self.elide(lifetime.range);
        }
        if let Some(&(_, index)) = self
            .late_names
            .iter()
            .rev()
            .find(|&&(name, _)| name == lifetime.name)
        {
            if index == u32::MAX {
                self.errors.push((
                    lifetime.range,
                    "bound output lifetime does not appear in the input types".to_owned(),
                ));
                return Region::Erased;
            }
            return Region::Late(index);
        }
        if let Some(i) = self.names.iter().rposition(|&n| n == Some(lifetime.name)) {
            return Region::Param(i as u32);
        }
        let message = if outer_lifetime(self.items, self.module, lifetime.name) {
            "can't use generic parameters from outer item".to_owned()
        } else {
            format!("use of undeclared lifetime name `{}`", lifetime.name)
        };
        self.errors.push((lifetime.range, message));
        Region::Erased
    }

    /// The region an elided lifetime at `range` is.
    fn elide(&mut self, range: TextRange) -> Region {
        match &self.elided {
            Elided::Fresh => {
                let index = self.names.len() + self.fresh.len();
                self.fresh.push(range);
                Region::Param(index as u32)
            }
            Elided::Is(r) => *r,
            Elided::Missing(help) => {
                let mut message = "missing lifetime specifier".to_owned();
                if let Some(help) = help {
                    message = format!("{message}\nhelp: {help}");
                }
                self.errors.push((range, message));
                Region::Erased
            }
            Elided::NotAllowed => {
                self.errors.push((
                    range,
                    "`&` without an explicit lifetime name cannot be used here".to_owned(),
                ));
                Region::Erased
            }
        }
    }

    fn walk(&mut self, written: &TypeRef, ty: Ty, out: &mut Vec<Region>) {
        let enclosing = self.ref_region.take();
        match (written, ty.kind()) {
            (_, TyKind::Error) => {}
            (
                TypeRef::Ref {
                    lifetime,
                    amp,
                    pointee,
                    ..
                },
                TyKind::Ref(_, inner),
            ) => {
                let r = match lifetime {
                    Some(l) if l.is_anonymous() => self.elide(l.range),
                    Some(l) => self.named(*l),
                    None => self.elide(*amp),
                };
                out.push(r);
                self.ref_region = Some(r);
                self.walk(pointee, inner, out);
                self.ref_region = None;
            }
            // A trait object's lifetime bound, as written, or rustc's
            // default: the reference it is behind, `'static` elsewhere.
            (TypeRef::Dyn(d), TyKind::Dynamic(object)) => {
                let r = match d.lifetime {
                    Some(l) if l.is_anonymous() => self.elide(l.range),
                    Some(l) => self.named(l),
                    None if self.elision_only => Region::Erased,
                    None if object_has_static_self(self.items, object.trait_) => Region::Static,
                    None => enclosing.unwrap_or(if self.infer_objects {
                        Region::Erased
                    } else {
                        Region::Static
                    }),
                };
                out.push(r);
                if let Some(written) = d.bounds.iter().find(|bound| {
                    self.items.resolve_trait(self.module, bound).0 == Some(object.trait_)
                }) {
                    self.object_arguments(&written.path, &written.bound_lifetimes, object, out);
                }
            }
            (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(_, inner))
            | (TypeRef::Array { elem: pointee, .. }, TyKind::Array(inner, _))
            | (TypeRef::Slice(pointee), TyKind::Slice(inner)) => self.walk(pointee, inner, out),
            (TypeRef::Tuple(elements), TyKind::Tuple(types)) => {
                for (e, &t) in elements.iter().zip(types) {
                    self.walk(e, t, out);
                }
            }
            (
                TypeRef::FnPtr {
                    bound_lifetimes,
                    params,
                    ret,
                    ..
                },
                TyKind::FnPtr(sig),
            ) => self.fn_ptr(bound_lifetimes, params, ret, sig, out),
            (TypeRef::Opaque(i), TyKind::Opaque(opaque)) => {
                match self.opaque_captures.get(*i as usize) {
                    Some(captures) => out.extend_from_slice(captures),
                    None => out.extend((0..opaque.lifetimes).map(|_| Region::Erased)),
                }
            }
            (TypeRef::Path(path), TyKind::Dynamic(object))
                if matches!(
                    self.items.resolve(self.module, path, crate::Ns::Types),
                    Ok(crate::Resolved::Def(crate::Def::Trait(_)))
                ) =>
            {
                out.push(if self.elision_only {
                    Region::Erased
                } else if object_has_static_self(self.items, object.trait_) {
                    Region::Static
                } else {
                    enclosing.unwrap_or(if self.infer_objects {
                        Region::Erased
                    } else {
                        Region::Static
                    })
                });
                self.object_arguments(path, &[], object, out);
            }
            (TypeRef::Path(path), _) => self.path(path, ty, out),
            (TypeRef::Infer, _) => {
                out.extend(std::iter::repeat_n(Region::Erased, slots(self.items, ty)))
            }
            _ => {}
        }
    }

    fn object_arguments(
        &mut self,
        path: &Path,
        names: &[Symbol],
        object: crate::DynTy,
        out: &mut Vec<Region>,
    ) {
        if let Some(signature) = object.quantified_signature() {
            if let Some((params, _)) = crate::objects::quantified_type_ref(self.items, path, object)
            {
                self.fn_ptr(
                    names,
                    &params,
                    &TypeRef::Tuple(Box::default()),
                    signature,
                    out,
                );
            } else {
                self.errors.push((
                    TextRange::default(),
                    "quantified object argument regions are not represented".into(),
                ));
            }
            return;
        }
        if crate::fn_trait_kind(self.items, object.trait_).is_some() {
            if let Some(signature) = object.callable_signature() {
                if let Some(TypeRef::FnPtr {
                    bound_lifetimes,
                    params,
                    ret,
                    ..
                }) = crate::objects::callable_type_ref(path, names)
                {
                    self.fn_ptr(&bound_lifetimes, &params, &ret, signature, out);
                } else {
                    out.extend(std::iter::repeat_n(
                        Region::Erased,
                        slots(self.items, Ty::new(TyKind::FnPtr(signature))),
                    ));
                }
            }
            return;
        }
        let start = out.len();
        let last = path.segments.len().saturating_sub(1);
        if !self.in_impl_header && path.args(last).is_none_or(|args| args.lifetimes.is_empty()) {
            let count = self.items.traits[object.trait_].generics.lifetimes.len();
            self.lifetime_args(path, "trait", count, out);
        } else {
            self.trait_lifetimes(path, object.trait_, self.in_impl_header, out);
        }
        let owner = out[start..].to_vec();
        let mut rows = Vec::new();
        let arguments = path.args(path.segments.len().saturating_sub(1));
        for (i, &ty) in object.args.iter().enumerate() {
            let start = out.len();
            if let Some(written) = arguments.and_then(|args| args.types.get(i)) {
                self.walk(written, ty, out);
            } else {
                out.extend(std::iter::repeat_n(Region::Erased, slots(self.items, ty)));
                if slots(self.items, ty) != 0 {
                    self.errors.push((
                        path.ranges.first().copied().unwrap_or_default(),
                        "object argument region rows are not supported yet".into(),
                    ));
                }
            }
            rows.push(out[start..].into());
        }
        let inherited = object::object_equalities(
            self.items,
            crate::TraitRef {
                trait_: object.trait_,
                name: object.name,
                args: object.args,
            },
            &owner,
            &rows,
        );
        // Names follow the declaring-trait order, not written equality order.
        for (&name, &ty) in object.assoc_names.iter().zip(object.assoc_tys) {
            if let Some(written) =
                arguments.and_then(|args| args.bindings.iter().find(|binding| binding.name == name))
            {
                self.walk(&written.ty, ty, out);
            } else if let Some((_, row)) = inherited.as_ref().and_then(|rows| {
                rows.iter()
                    .find(|(projection, _)| projection.alias.name == name)
            }) {
                out.extend_from_slice(row);
            } else {
                out.extend(std::iter::repeat_n(Region::Erased, slots(self.items, ty)));
                if slots(self.items, ty) != 0 {
                    self.errors.push((
                        path.ranges.first().copied().unwrap_or_default(),
                        "object equality region rows are not supported yet".into(),
                    ));
                }
            }
        }
    }

    /// Bound lifetimes belong to the pointer signature. Only free lifetimes
    /// and captured generic storage contribute slots to the containing value.
    fn fn_ptr(
        &mut self,
        names: &[Symbol],
        params: &[TypeRef],
        ret: &TypeRef,
        sig: crate::FnSigTy,
        out: &mut Vec<Region>,
    ) {
        for (i, &name) in names.iter().enumerate() {
            if self.names.contains(&Some(name))
                || self.late_names.iter().any(|&(n, _)| n == name)
                || names[..i].contains(&name)
            {
                self.errors.push((
                    TextRange::default(),
                    "lifetime name shadows a lifetime already in scope".into(),
                ));
            }
        }
        let description = crate::fn_regions::describe(
            self.items,
            crate::fn_regions::Scope {
                module: self.module,
                self_impl: self.scope.self_impl,
            },
            names,
            params,
            ret,
            sig,
            &mut self.errors,
        );
        for lifetime in description.free {
            out.push(self.named(lifetime));
        }
        for (written, ty) in description.captures {
            out.extend(self.regions(&written, ty));
        }
    }

    fn path(&mut self, path: &Path, ty: Ty, out: &mut Vec<Region>) {
        // A named alias can resolve to a projection whose input row grows
        // after substitution. Walk the alias arguments in its own declaration
        // scope, rather than interpret its spelling as a qualified projection.
        if path.qself.is_none()
            && matches!(ty.kind(), TyKind::Alias(_))
            && !(path.segments.len() == 1 && self.scope.generics.contains(&path.segments[0]))
        {
            let alias = match self.items.resolve(self.module, path, Ns::Types) {
                Ok(Resolved::Def(Def::TypeAlias(id))) => Some(id),
                Err(flux_hir::def_map::ResolveError::Private {
                    resolved: Resolved::Def(Def::TypeAlias(id)),
                    ..
                }) => Some(id),
                _ => None,
            };
            if let Some(id) = alias {
                self.alias(path, id, ty, out);
                return;
            }
        }
        if path.qself.is_some()
            && let Some(sigs) = self.signatures
        {
            let raw = self
                .inferred_root_alias
                .take()
                .map(Ty::alias)
                .unwrap_or_else(|| {
                    crate::resolve_type(
                        self.items,
                        self.module,
                        &TypeRef::Path(path.clone()),
                        self.scope,
                        path.range(0, TextRange::default()),
                        &mut Vec::new(),
                    )
                });
            let raw = match raw.kind() {
                TyKind::Alias(alias) if self.body && raw != ty => {
                    Ty::alias(self.normalize_written_projection_inputs(path, sigs, alias))
                }
                _ => raw,
            };
            if self.body
                && let TyKind::Alias(alias) = raw.kind()
                && raw != ty
                && selected::closed_type_projection(self.items, sigs, alias, ty)
            {
                let mut inputs = Vec::new();
                self.projection_inputs(path, alias, &mut inputs);
                if inputs.len() == alias.family_lifetimes as usize {
                    return;
                }
            }
            if self.body
                && let TyKind::Alias(alias) = raw.kind()
                && raw != ty
                && selected::generic_zero_row_projection(
                    self.items,
                    sigs,
                    alias,
                    ty,
                    self.scope,
                    self.caller_projections,
                )
            {
                let mut inputs = Vec::new();
                self.projection_inputs(path, alias, &mut inputs);
                if inputs.is_empty() {
                    return;
                }
            }
            if let TyKind::Alias(alias) = raw.kind()
                && raw != ty
                && !has_declared_projection_inputs(alias)
                && alias.args.iter().all(|&input| {
                    slots(self.items, input) == 0 && !input.has_aliases() && !input.has_opaques()
                })
                && slots(self.items, ty) == 0
                && !ty.has_aliases()
                && !ty.has_opaques()
                && self
                    .caller_projections
                    .iter()
                    .any(|p| p.alias == alias && p.ty == ty)
                && self.scope.bounds.contains(&crate::Predicate {
                    self_ty: alias.args[0],
                    trait_ref: crate::TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: &alias.args[1..],
                    },
                })
            {
                // This exact caller equality supplies the abstract output type;
                // no impl or output lifetime is inferred from empty input rows.
                // A type parameter remains a parameter after this substitution.
                let mut inputs = Vec::new();
                self.projection_inputs(path, alias, &mut inputs);
                if inputs.is_empty() {
                    return;
                }
            }
            if let TyKind::Alias(alias) = raw.kind()
                && (!has_declared_projection_inputs(alias)
                    || (self.body && is_closed_region_free(self.items, ty)))
                && flux_solve::traits::normalize(self.items, sigs, &[], raw) == ty
                && raw != ty
            {
                let mut inputs = Vec::new();
                self.projection_inputs(path, alias, &mut inputs);
                let row = if self.body && is_closed_region_free(self.items, ty) {
                    selected::closed_projection(self.items, sigs, alias, &inputs, ty)
                } else {
                    selected::projection(self.items, sigs, alias, &inputs)
                };
                if let Some(row) = row {
                    self.selected_inputs.push((alias, inputs.into()));
                    out.extend(row);
                } else {
                    self.errors.push((
                        path.range(0, TextRange::default()),
                        "selected projection lifetime mapping is not supported yet".into(),
                    ));
                }
                return;
            }
            if raw != ty {
                self.errors.push((
                    path.range(0, TextRange::default()),
                    "selected projection lifetime mapping is not supported yet".into(),
                ));
                return;
            }
        }
        // `Self::Out` can resolve to a projection written in this impl's
        // associated type value. Its input row belongs to that definition.
        // When it still names the impl's own trait projection, use that
        // occurrence's inputs, even if the definition has the same slot count.
        if let TyKind::Alias(output) = ty.kind()
            && path.qself.is_none()
            && let [first, name] = &*path.segments
            && *first == kw::SelfUpper
            && path.args(1).is_none()
            && self.scope.self_types.contains(&(*name, ty))
            && let Some((_, row)) = self
                .self_types
                .iter()
                .find(|(candidate, _)| candidate == name)
            && row.len() == slots(self.items, ty)
        {
            self.check_args(path);
            if !self.elision_only {
                out.extend_from_slice(row);
            }
            if let Some(selections) = &mut self.selections
                && let Some((_, associated)) = self
                    .self_type_selections
                    .iter()
                    .find(|(candidate, _)| candidate == name)
            {
                // Only replay an input from the associated definition when this
                // occurrence has resolved to that projection already. If the
                // method signature still names its own trait's alias, its input
                // row belongs to that alias.
                selections.extend(
                    associated
                        .iter()
                        .filter(|selection| {
                            selection.alias == output
                                && selection.qualified
                                && !has_declared_projection_inputs(selection.alias)
                                && selection.args.iter().any(|row| !row.is_empty())
                        })
                        .cloned()
                        .map(|mut selection| {
                            selection.qualified = false;
                            selection
                        }),
                );
            }
            return;
        }
        if let TyKind::Alias(alias) = ty.kind()
            && has_projection_inputs(self.items, alias)
        {
            let start = out.len();
            self.projection_inputs(path, alias, out);
            if out.len() - start != slots(self.items, ty) {
                self.errors.push((
                    path.range(0, TextRange::default()),
                    "projection lifetime inputs do not match this occurrence".into(),
                ));
                return;
            }
            if (!self.qualified_legacy_selections || !has_declared_projection_inputs(alias))
                && let Some(selections) = self.selections.as_mut()
            {
                let row = &out[start..];
                let mut offset = alias.owner_lifetimes as usize;
                let args = alias
                    .args
                    .iter()
                    .map(|&ty| {
                        let len = slots(self.items, ty);
                        let regions = row[offset..offset + len].into();
                        offset += len;
                        regions
                    })
                    .collect();
                selections.push(ProjectionSelection {
                    alias,
                    qualified: path.qself.is_some(),
                    lifetimes: row[..alias.owner_lifetimes as usize].into(),
                    args,
                });
            }
            return;
        }
        if self.body && path.qself.is_some() {
            let mut found = Vec::new();
            lifetimes_in(&TypeRef::Path(path.clone()), &mut found);
            if let Some(l) = found.iter().find(|l| !l.is_anonymous()) {
                self.errors
                    .push((l.range, BODY_PROJECTION_LIFETIMES.to_owned()));
            }
        }
        if let TyKind::Alias(alias) = ty.kind()
            && self.selections.is_some()
            && (!self.qualified_legacy_selections || path.qself.is_some())
        {
            // Missing syntax-to-slot mappings stay unknown. In particular,
            // never recover Self's lifetimes by comparing erased types: an
            // explicitly written instance may have different lifetimes.
            let mut args: Vec<Box<[Region]>> = alias
                .args
                .iter()
                .map(|&t| vec![Region::Erased; slots(self.items, t)].into())
                .collect();
            if let Some(q) = &path.qself {
                args[0] = self.regions(&q.ty, alias.args[0]);
            } else if path.segments.first() == Some(&kw::SelfUpper)
                && let Some(regions) = self.self_regions
            {
                args[0] = regions.into();
            }
            self.selections.as_mut().unwrap().push(ProjectionSelection {
                alias,
                qualified: path.qself.is_some(),
                lifetimes: Box::default(),
                args: args.into(),
            });
        }
        if matches!(ty.kind(), TyKind::Param(_) | TyKind::Alias(_)) {
            self.check_args(path);
            return;
        }
        // `Self::Out` or `<Self as Tr>::Out`, which the type resolved to
        // the block's own (otherwise it is an associated type).
        let is_self = |p: &Path| p.as_name() == Some(kw::SelfUpper);
        let own = match (&path.qself, &*path.segments) {
            (None, [first, name]) if *first == kw::SelfUpper => Some(*name),
            (Some(q), [name]) if matches!(&q.ty, TypeRef::Path(p) if is_self(p)) => Some(*name),
            _ => None,
        };
        if path.qself.is_none() && own.is_some_and(|name| self.unmapped_self_types.contains(&name))
        {
            self.errors.push((
                path.range(0, TextRange::default()),
                "regions in inherited associated types are not supported for this impl".into(),
            ));
            return;
        }
        if let Some(name) = own
            && let Some((_, regions)) = self.self_types.iter().find(|(n, _)| *n == name)
        {
            self.check_args(path);
            if !self.elision_only || path.qself.is_some() {
                out.extend_from_slice(regions);
            }
            return;
        }
        if path.qself.is_none()
            && let Some(name) = own
            && let Some((_, regions)) = self.inherited_self_types.iter().find(|(n, _)| *n == name)
        {
            self.check_args(path);
            if !self.elision_only || path.qself.is_some() {
                out.extend_from_slice(regions);
            }
            return;
        }
        let resolved = match self.items.resolve(self.module, path, Ns::Types) {
            Ok(r) => r,
            Err(flux_hir::def_map::ResolveError::Private { resolved, .. }) => resolved,
            Err(_) => {
                if self.body && path.qself.is_none() {
                    let mut found = Vec::new();
                    lifetimes_in(&TypeRef::Path(path.clone()), &mut found);
                    if let Some(l) = found.iter().find(|l| !l.is_anonymous()) {
                        self.errors
                            .push((l.range, BODY_PROJECTION_LIFETIMES.to_owned()));
                    }
                }
                return;
            }
        };
        let last = path.segments.len().saturating_sub(1);
        let args = path.args(last);
        match resolved {
            Resolved::SelfType => {
                if self.elision_only {
                    return;
                }
                if let Some(regions) = self.self_regions {
                    out.extend_from_slice(regions);
                }
            }
            Resolved::Def(Def::Struct(id)) => {
                let def = &self.items.structs[id];
                let kind = if def.is_enum() { "enum" } else { "struct" };
                let want = def.generics.lifetimes.len();
                let start = out.len();
                self.lifetime_args(path, kind, want, out);
                let lifetimes = out[start..].to_vec();
                if let TyKind::Adt(_, types) = ty.kind() {
                    let mut type_regions = Vec::new();
                    for (i, &t) in types.iter().enumerate() {
                        let mut regions = Vec::new();
                        if let Some(written) = args.and_then(|a| a.types.get(i)) {
                            self.type_argument_regions(
                                written,
                                t,
                                &def.generics,
                                i,
                                &lifetimes,
                                &mut regions,
                            );
                        } else if let Some((written, range)) =
                            def.generics.params.get(i).and_then(|p| p.default.as_ref())
                            && !def.generics.params[i].is_const()
                        {
                            regions = self.default_regions(
                                def.loc.module,
                                &def.generics,
                                written,
                                *range,
                                &lifetimes,
                                &type_regions,
                            );
                        }
                        regions.resize(slots(self.items, t), Region::Erased);
                        out.extend_from_slice(&regions);
                        type_regions.push(regions);
                    }
                }
            }
            Resolved::Def(Def::TypeAlias(id)) => self.alias(path, id, ty, out),
            _ => self.check_args(path),
        }
    }

    /// A direct object argument inherits an explicit bound on the containing
    /// type's parameter. This is distinct from inference in an unbounded Box.
    fn type_argument_regions(
        &mut self,
        written: &TypeRef,
        ty: Ty,
        generics: &flux_hir::Generics,
        index: usize,
        lifetimes: &[Region],
        out: &mut Vec<Region>,
    ) {
        let previous = self.ref_region.take();
        if let TypeRef::Dyn(object) = written
            && object.lifetime.is_none()
            && let Some(parameter) = generics.params.get(index)
        {
            let mut bounds = Vec::new();
            for bound in &generics.type_outlives {
                if matches!(&bound.ty,TypeRef::Path(path) if path.as_name()==Some(parameter.name))
                    && !bounds
                        .iter()
                        .any(|l: &LifetimeRef| l.name == bound.lifetime.name)
                {
                    bounds.push(bound.lifetime);
                }
            }
            if bounds.iter().any(|bound| bound.is_static()) {
                self.ref_region = Some(Region::Static);
            } else if let [bound] = bounds.as_slice() {
                let region = generics
                    .lifetimes
                    .iter()
                    .position(|l| l.name == bound.name)
                    .and_then(|i| lifetimes.get(i))
                    .copied()
                    .unwrap_or(Region::Erased);
                if region == Region::Erased {
                    self.errors.push((
                        object.range,
                        "inferred containing-type object lifetimes are not supported yet".into(),
                    ));
                }
                self.ref_region = Some(region);
            } else if !bounds.is_empty() && !self.infer_objects {
                self.errors
                    .push((object.range, "ambiguous object lifetime bound".into()));
            }
        }
        self.walk(written, ty, out);
        self.ref_region = previous;
    }

    pub(crate) fn add_projection_bounds(&mut self, generics: &Generics) {
        for predicate in &generics.predicates {
            let mut scratch = Vec::new();
            let subject = crate::resolve_type(
                self.items,
                self.module,
                &predicate.ty,
                self.scope,
                predicate.ty_range,
                &mut scratch,
            );
            if let (Some(trait_), _) = self.items.resolve_trait(self.module, &predicate.bound) {
                self.add_projection_owner(subject, trait_, &predicate.bound);
            }
        }
    }

    fn add_trait_projection_bounds(&mut self, owner: flux_hir::TraitId) {
        let declaration = &self.items.traits[owner];
        let subject = Ty::param(0, kw::SelfUpper);
        self.in_module(declaration.loc.module, |walker| {
            for bound in &declaration.supertraits {
                if let (Some(trait_), _) = walker.items.resolve_trait(walker.module, bound) {
                    walker.add_projection_owner(subject, trait_, bound);
                }
            }
        });
    }

    fn add_projection_owner(
        &mut self,
        subject: Ty,
        trait_: flux_hir::TraitId,
        bound: &flux_hir::Bound,
    ) {
        self.projection_owners.push(ProjectionOwner {
            subject,
            trait_,
            module: self.module,
            bound: bound.clone(),
        });
    }

    fn in_module<T>(&mut self, module: ModuleId, f: impl FnOnce(&mut Self) -> T) -> T {
        let previous = std::mem::replace(&mut self.module, module);
        let value = f(self);
        self.module = previous;
        value
    }

    /// A selected input contributes the regions of its output, after its own
    /// occurrence has been checked. Normalize through written wrappers so
    /// `projection_inputs` can walk each original spelling with the
    /// corresponding normalized type.
    fn normalize_written_projection_inputs(
        &self,
        path: &Path,
        sigs: &Signatures,
        alias: crate::AliasTy,
    ) -> crate::AliasTy {
        let Some(qself) = &path.qself else {
            return alias;
        };
        if !alias
            .args
            .iter()
            .chain(alias.family_args)
            .any(|ty| ty.has_aliases())
        {
            return alias;
        }
        let owner = qself.trait_.as_ref().filter(|_| !qself.associated_bound);
        let owner_args = owner.and_then(|path| path.args(path.segments.len().saturating_sub(1)));
        let family_args = path.args(path.segments.len().saturating_sub(1));
        let normalize = |ty: Ty, written: Option<&TypeRef>| {
            written.map_or(ty, |written| {
                self.normalize_written_projection_input(sigs, written, ty)
            })
        };
        let self_ty = normalize(alias.args[0], Some(&qself.ty));
        let args =
            std::iter::once(self_ty).chain(alias.args[1..].iter().enumerate().map(|(i, &ty)| {
                let written = owner_args.and_then(|args| args.types.get(i));
                if written.is_none()
                    && self_ty != alias.args[0]
                    && ty == alias.args[0]
                    && owner.is_some()
                    && resolve::trait_default_self(self.items, alias.trait_, i)
                {
                    self_ty
                } else {
                    normalize(ty, written)
                }
            }));
        crate::AliasTy {
            args: Ty::list(&args.collect::<Vec<_>>()),
            family_args: Ty::list(
                &alias
                    .family_args
                    .iter()
                    .enumerate()
                    .map(|(i, &ty)| normalize(ty, family_args.and_then(|args| args.types.get(i))))
                    .collect::<Vec<_>>(),
            ),
            ..alias
        }
    }

    fn normalize_written_projection_input(
        &self,
        sigs: &Signatures,
        written: &TypeRef,
        ty: Ty,
    ) -> Ty {
        if !ty.has_aliases() {
            return ty;
        }
        match (written, ty.kind()) {
            (TypeRef::Path(path), _) if path.qself.is_some() => {
                flux_solve::traits::normalize(self.items, sigs, &[], ty)
            }
            (TypeRef::Ref { pointee, .. }, TyKind::Ref(mutability, inner)) => Ty::new(TyKind::Ref(
                mutability,
                self.normalize_written_projection_input(sigs, pointee, inner),
            )),
            _ => ty,
        }
    }

    /// Resolve a projection's inputs from this exact syntax occurrence. Never
    /// infer trait lifetimes by comparing only erased self/argument types.
    fn projection_inputs(&mut self, path: &Path, alias: crate::AliasTy, out: &mut Vec<Region>) {
        let owner_path = path
            .qself
            .as_ref()
            .and_then(|q| q.trait_.as_ref())
            .filter(|owner| {
                self.items
                    .resolve_trait(
                        self.module,
                        &flux_hir::Bound {
                            path: (*owner).clone(),
                            bound_lifetimes: Box::default(),
                            range: TextRange::default(),
                        },
                    )
                    .0
                    == Some(alias.trait_)
            })
            .map(|path| (self.module, path.clone()));
        let explicit_owner = owner_path.is_some();
        // Lifetimes recovered from a declaration or an impl's Self
        // identity are not written dependencies of an input type.
        let infer_owner = !self.elision_only;
        let candidates: Vec<_> = self
            .projection_owners
            .iter()
            .filter(|owner| {
                infer_owner
                    && owner.subject == alias.args[0]
                    && owner.trait_ == alias.trait_
                    // An inferred occurrence can instantiate a higher-ranked
                    // bound, but its binder is not a named body lifetime.
                    && (!self.infer_unknown_owners || owner.bound.bound_lifetimes.is_empty())
            })
            .map(|owner| (owner.module, &owner.bound.path))
            .collect();
        let owner_path = owner_path.or_else(|| {
            if candidates.windows(2).all(|pair| pair[0] == pair[1]) {
                candidates
                    .first()
                    .map(|&(module, path)| (module, path.clone()))
            } else {
                None
            }
        });
        let written_self = match &path.qself {
            Some(q) => matches!(&q.ty, TypeRef::Path(p) if p.as_name() == Some(kw::SelfUpper)),
            None => path.segments.first() == Some(&kw::SelfUpper),
        };
        let reuse_self_owner = infer_owner
            && !explicit_owner
            && written_self
            && self.scope.self_ty == Some(alias.args[0])
            && self
                .self_projection_owner
                .as_ref()
                .is_some_and(|(trait_, lifetimes)| {
                    *trait_ == alias.trait_ && lifetimes.len() == alias.owner_lifetimes as usize
                });
        let inherited = if infer_owner && owner_path.is_none() && !reuse_self_owner {
            self.inherited_projection_inputs(alias)
        } else {
            None
        };
        if alias.owner_lifetimes != 0 {
            if reuse_self_owner && let Some((_, lifetimes)) = &self.self_projection_owner {
                // An implicit Self owner reuses the resolved impl header.
                // In particular, its anonymous lifetime is not elided again
                // under an associated output or method return's elision scope.
                out.extend(lifetimes.iter().copied());
            } else if let Some((lifetimes, _)) = &inherited {
                out.extend(lifetimes.iter().copied());
            } else if let Some((module, owner)) = &owner_path {
                self.in_module(*module, |walker| {
                    if explicit_owner {
                        // A qualified projection is a type occurrence. Its
                        // omitted lifetime participates in type elision,
                        // including the output of a parenthesized Fn bound.
                        walker.lifetime_args(owner, "trait", alias.owner_lifetimes as usize, out);
                    } else {
                        walker.trait_lifetimes(owner, alias.trait_, false, out);
                    }
                });
            } else if infer_owner
                && let Some((trait_, lifetimes)) = &self.self_projection_owner
                && *trait_ == alias.trait_
                && self.scope.self_ty == Some(alias.args[0])
                && lifetimes.len() == alias.owner_lifetimes as usize
            {
                out.extend(lifetimes.iter().copied());
            } else if self.infer_unknown_owners || !infer_owner {
                out.extend(std::iter::repeat_n(
                    Region::Erased,
                    alias.owner_lifetimes as usize,
                ));
            } else {
                self.errors.push((
                    path.range(0, TextRange::default()),
                    "projection owner lifetimes are not represented at this occurrence".into(),
                ));
                out.extend(std::iter::repeat_n(
                    Region::Erased,
                    alias.owner_lifetimes as usize,
                ));
            }
        }
        let self_ty = alias.args[0];
        let self_start = out.len();
        if let Some(qself) = &path.qself {
            self.walk(&qself.ty, self_ty, out);
        } else if !self.elision_only && path.segments.first() == Some(&kw::SelfUpper) {
            if let Some(regions) = self.self_regions {
                out.extend(regions.iter().copied());
            } else {
                out.extend(std::iter::repeat_n(
                    Region::Erased,
                    slots(self.items, self_ty),
                ));
            }
        } else {
            // A generic Self has no own slots; a concrete written self needs
            // its syntax table and cannot borrow a different bound's table.
            out.extend(std::iter::repeat_n(
                Region::Erased,
                slots(self.items, self_ty),
            ));
        }
        let self_regions = out[self_start..].to_vec();
        for (index, &ty) in alias.args.iter().skip(1).enumerate() {
            if let Some((module, written)) = owner_path.as_ref().and_then(|(module, path)| {
                let written = path
                    .args(path.segments.len().saturating_sub(1))?
                    .types
                    .get(index)?;
                Some((*module, written))
            }) {
                self.in_module(module, |walker| walker.walk(written, ty, out));
            } else if owner_path.is_some()
                && ty == self_ty
                && resolve::trait_default_self(self.items, alias.trait_, index)
            {
                // An omitted `Rhs = Self` repeats this occurrence's regions,
                // including explicit lifetimes on the qualified Self type.
                out.extend(self_regions.iter().copied());
            } else if let Some(row) = inherited
                .as_ref()
                .and_then(|(_, arguments)| arguments.get(index))
                .filter(|row| row.len() == slots(self.items, ty))
            {
                out.extend(row.iter().copied());
            } else {
                out.extend(std::iter::repeat_n(Region::Erased, slots(self.items, ty)));
            }
        }
        self.lifetime_args(
            path,
            "associated type",
            alias.family_lifetimes as usize,
            out,
        );
        let last = path.segments.len().saturating_sub(1);
        for (index, &ty) in alias.family_args.iter().enumerate() {
            if let Some(written) = path.args(last).and_then(|a| a.types.get(index)) {
                self.walk(written, ty, out);
            } else {
                out.extend(std::iter::repeat_n(Region::Erased, slots(self.items, ty)));
            }
        }
    }

    fn inherited_projection_inputs(
        &mut self,
        alias: crate::AliasTy,
    ) -> Option<object::ProjectionInputs> {
        if self.inherited_projection_stack.contains(&alias)
            || self.inherited_projection_stack.len() >= 64
        {
            return None;
        }
        self.inherited_projection_stack.push(alias);
        let result = (|| {
            let sources: Vec<_> = self
                .projection_owners
                .iter()
                .filter(|owner| {
                    owner.subject == alias.args[0]
                        && owner.trait_ != alias.trait_
                        && owner.bound.bound_lifetimes.is_empty()
                })
                .cloned()
                .collect();
            let target = crate::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: Ty::list(&alias.args[1..]),
            };
            let mut found = None;
            for ProjectionOwner {
                subject,
                trait_,
                module,
                bound,
            } in sources
            {
                let input = self.in_module(module, |walker| {
                    let mut errors = Vec::new();
                    let (principal, _) = crate::traits::resolve_bound(
                        walker.items,
                        module,
                        &bound,
                        subject,
                        walker.scope,
                        &mut errors,
                    )?;
                    if !errors.is_empty() {
                        return None;
                    }
                    let mut lifetimes = Vec::new();
                    walker.trait_lifetimes(&bound.path, trait_, false, &mut lifetimes);
                    let written = bound.path.args(bound.path.segments.len().saturating_sub(1));
                    let mut rows = Vec::new();
                    for (index, &ty) in principal.args.iter().enumerate() {
                        if let Some(written) = written.and_then(|args| args.types.get(index)) {
                            rows.push(walker.regions(written, ty));
                        } else if slots(walker.items, ty) == 0 {
                            rows.push(Box::default());
                        } else {
                            return None;
                        }
                    }
                    object::inherited_inputs(
                        walker.items,
                        subject,
                        principal,
                        &lifetimes,
                        &rows,
                        target,
                    )
                });
                let Some(input) = input else {
                    continue;
                };
                if found.as_ref().is_some_and(|previous| previous != &input) {
                    return None;
                }
                found = Some(input);
            }
            found
        })();
        self.inherited_projection_stack.pop();
        result
    }

    /// The lifetime arguments of `path`, a `kind` with `want` lifetime
    /// parameters: those written, or elided ones.
    fn lifetime_args(&mut self, path: &Path, kind: &str, want: usize, out: &mut Vec<Region>) {
        let last = path.segments.len().saturating_sub(1);
        let whole = path.ranges.get(last).copied().unwrap_or_default();
        let written: &[LifetimeRef] = path.args(last).map_or(&[], |a| &a.lifetimes);
        if written.is_empty() {
            // rustc points after the name, where they would go.
            let at = match path.args(last) {
                Some(args) => TextRange::new(args.range.start(), args.range.start() + 1),
                None => path.range(last, whole),
            };
            for _ in 0..want {
                let r = self.elide(at);
                out.push(r);
            }
            return;
        }
        if written.len() != want {
            let verb = if written.len() == 1 { "was" } else { "were" };
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            self.errors.push((
                path.range(last, whole),
                format!(
                    "{kind} takes {want} lifetime argument{} but {} lifetime argument{} {verb} supplied",
                    plural(want),
                    written.len(),
                    plural(written.len())
                ),
            ));
            out.extend(std::iter::repeat_n(Region::Erased, want));
            return;
        }
        for &l in written {
            let r = self.named(l);
            out.push(r);
        }
    }

    /// Resolve a trait path against its declaration rather than erased types.
    /// Omission is forbidden in impl headers; bounds require named arguments.
    fn trait_lifetimes(
        &mut self,
        path: &Path,
        trait_: flux_hir::TraitId,
        header: bool,
        out: &mut Vec<Region>,
    ) {
        let want = self.items.traits[trait_].generics.lifetimes.len();
        let last = path.segments.len().saturating_sub(1);
        let written = path.args(last).map_or(&[][..], |a| &a.lifetimes);
        if want != 0 && written.is_empty() {
            if self.body && self.infer_unknown_owners && !header {
                out.extend(std::iter::repeat_n(Region::Erased, want));
                return;
            }
            let at = path.ranges.get(last).copied().unwrap_or_default();
            self.errors.push((
                at,
                if header {
                    "implicit elided lifetime not allowed here"
                } else {
                    "missing lifetime specifier"
                }
                .to_owned(),
            ));
            out.extend(std::iter::repeat_n(Region::Erased, want));
            return;
        }
        // Resolve names even when arity is wrong, so undeclared names are
        // never hidden behind an argument-count diagnostic.
        if written.len() != want {
            for &l in written {
                self.named(l);
            }
        }
        self.lifetime_args(path, "trait", want, out);
    }

    /// Lifetime arguments where there are no lifetime parameters
    /// (E0107), and those in the type arguments, checked.
    fn check_args(&mut self, path: &Path) {
        for (i, args) in path.args.iter().enumerate() {
            let Some(args) = args else { continue };
            if let Some(&first) = args.lifetimes.first() {
                let n = args.lifetimes.len();
                let what = if i + 1 == path.segments.len() {
                    "type"
                } else {
                    "item"
                };
                self.errors.push((
                    first.range,
                    format!(
                        "{what} takes 0 lifetime arguments but {n} lifetime argument{} {} supplied",
                        if n == 1 { "" } else { "s" },
                        if n == 1 { "was" } else { "were" }
                    ),
                ));
            }
            self.check_written(args);
        }
    }

    /// The lifetimes written in `args`' types, without types to match:
    /// each must be in scope.
    fn check_written(&mut self, args: &GenericArgs) {
        let mut found = Vec::new();
        for t in args.types.iter() {
            lifetimes_in(t, &mut found);
        }
        for l in found {
            if !l.is_anonymous() {
                self.named(l);
            }
        }
    }

    /// Instantiate an omitted struct/alias default in its declaration scope.
    /// A default may carry declared lifetimes or repeat an earlier type's slots.
    fn default_regions(
        &mut self,
        module: ModuleId,
        params: &flux_hir::Generics,
        written: &TypeRef,
        range: TextRange,
        lifetimes: &[Region],
        types: &[Vec<Region>],
    ) -> Vec<Region> {
        let names = params.lifetimes.iter().map(|l| Some(l.name)).collect();
        let generics = crate::generic_names(self.items, None, params);
        let consts = crate::generic_consts(self.items, module, None, params);
        let scope = TypeScope {
            generics: &generics,
            consts: &consts,
            defaults: self.scope.defaults,
            lens: self.scope.lens,
            struct_fields: self.scope.struct_fields,
            ..TypeScope::default()
        };
        let generic =
            crate::resolve_type(self.items, module, written, scope, range, &mut Vec::new());
        let mut walker = Walker::new(self.items, module, names);
        walker.scope = scope;
        let table = walker.regions(written, generic);
        self.errors.append(&mut walker.errors);
        let mut map = |r| match r {
            Region::Param(i) => lifetimes.get(i as usize).copied().unwrap_or(Region::Erased),
            r => r,
        };
        let args = |j: u32| types.get(j as usize).cloned().unwrap_or_default();
        let mut out = Vec::new();
        instantiate(self.items, generic, &table, &mut map, &args, &mut out);
        out
    }

    /// Type alias `id` named by `path`: its slots with its parameters'
    /// substituted.
    fn alias(&mut self, path: &Path, id: TypeAliasId, ty: Ty, out: &mut Vec<Region>) {
        let def = &self.items.type_aliases[id];
        let last = path.segments.len().saturating_sub(1);
        let mut lifetimes = Vec::new();
        self.lifetime_args(
            path,
            "type alias",
            def.generics.lifetimes.len(),
            &mut lifetimes,
        );
        let table = alias_regions_in(self.items, id, &mut self.errors, self.body);
        let Some(alias_ty) = table.ty else { return };
        // Match against the resolved instance rather than resolving `_` again:
        // an inferred argument's slots must precede the alias's later slots.
        fn arguments(generic: Ty, instance: Ty, out: &mut Vec<Option<Ty>>) {
            if let TyKind::Param(p) = generic.kind() {
                let i = p.index as usize;
                if out.len() <= i {
                    out.resize(i + 1, None);
                }
                out[i] = Some(instance);
            } else {
                for (g, t) in generic.children().zip(instance.children()) {
                    arguments(g, t, out);
                }
            }
        }
        let mut resolved = Vec::new();
        arguments(alias_ty, ty, &mut resolved);
        let mut types: Vec<Vec<Region>> = Vec::new();
        for (i, param) in def.generics.params.iter().enumerate() {
            let mut regions = Vec::new();
            if let Some((written, &range)) = path
                .args(last)
                .and_then(|a| a.types.get(i).zip(a.ranges.get(i)))
            {
                let t = resolved.get(i).copied().flatten().unwrap_or_else(|| {
                    crate::resolve_type(
                        self.items,
                        self.module,
                        written,
                        self.scope,
                        range,
                        &mut Vec::new(),
                    )
                });
                self.type_argument_regions(written, t, &def.generics, i, &lifetimes, &mut regions);
            } else if !param.is_const()
                && let Some((written, range)) = &param.default
            {
                regions = self.default_regions(
                    def.loc.module,
                    &def.generics,
                    written,
                    *range,
                    &lifetimes,
                    &types,
                );
            }
            types.push(regions);
        }
        let mut map = |r: Region| match r {
            Region::Param(i) => lifetimes.get(i as usize).copied().unwrap_or(Region::Erased),
            r => r,
        };
        let args = |j: u32| types.get(j as usize).cloned().unwrap_or_default();
        if self.expand_alias_selections && self.selections.is_some() && self.errors.is_empty() {
            // Alias expansion must forward the selected projection's inputs,
            // even when its eventual concrete output has no lifetime slots.
            let names: Vec<_> = def.generics.params.iter().map(|p| p.name).collect();
            let mut nested = Walker::new(
                self.items,
                def.loc.module,
                def.generics
                    .lifetimes
                    .iter()
                    .map(|l| Some(l.name))
                    .collect(),
            );
            nested.scope = TypeScope {
                generics: &names,
                signature_of: Some("type aliases"),
                ..TypeScope::default()
            };
            nested.elided = Elided::Missing(None);
            nested.selections = Some(Vec::new());
            nested.expand_alias_selections = true;
            nested.regions(&def.ty, alias_ty);
            self.errors.append(&mut nested.errors);
            let selections = nested.selections.take().unwrap_or_default();
            if !selections.is_empty() {
                let actual =
                    if let Some(actual) = resolved.iter().copied().collect::<Option<Vec<_>>>() {
                        actual
                    } else {
                        let mut actual = path.args(last).map_or_else(Vec::new, |written| {
                            crate::resolve::resolve_generic_args(
                                self.items,
                                self.module,
                                def.loc.module,
                                &def.generics.params,
                                written,
                                self.scope,
                                &mut self.errors,
                            )
                        });
                        crate::resolve::fill_type_defaults(
                            self.items,
                            Def::TypeAlias(id),
                            &mut actual,
                            self.scope,
                            &mut self.errors,
                        );
                        for (index, known) in resolved.iter().enumerate() {
                            if let (Some(known), Some(argument)) = (known, actual.get_mut(index)) {
                                *argument = *known;
                            }
                        }
                        actual
                    };
                for mut selection in selections {
                    let mut rows = Vec::new();
                    for (&ty, row) in selection.alias.args.iter().zip(selection.args.iter()) {
                        let mut mapped = Vec::new();
                        instantiate(self.items, ty, row, &mut map, &args, &mut mapped);
                        rows.push(mapped.into_boxed_slice());
                    }
                    for lifetime in &mut selection.lifetimes {
                        *lifetime = map(*lifetime);
                    }
                    selection.alias = selection.alias.map_args(|ty| ty.subst(&actual));
                    selection.args = rows.into();
                    if self.qualified_legacy_selections {
                        // The alias definition supplies declaration proofs.
                        // Its spelling did not occur in the method signature.
                        selection.qualified = false;
                    }
                    if let Some(selections) = self.selections.as_mut() {
                        selections.push(selection);
                    }
                }
            }
        }
        instantiate(self.items, alias_ty, &table.regions, &mut map, &args, out);
    }
}

/// Type alias `id`'s type and slots in terms of its parameters, with the
/// errors in its lifetimes.
pub(crate) fn alias_regions(
    items: &ItemTree,
    id: TypeAliasId,
    errors: &mut Vec<(TextRange, String)>,
) -> AliasRegions {
    alias_regions_in(items, id, errors, false)
}

fn alias_regions_in(
    items: &ItemTree,
    id: TypeAliasId,
    errors: &mut Vec<(TextRange, String)>,
    body: bool,
) -> AliasRegions {
    thread_local! {
        static ACTIVE: std::cell::RefCell<Vec<TypeAliasId>> = const { std::cell::RefCell::new(Vec::new()) };
    }
    if ACTIVE.with(|a| a.borrow().contains(&id)) {
        return AliasRegions::default();
    }
    ACTIVE.with(|a| a.borrow_mut().push(id));
    let def: &TypeAlias = &items.type_aliases[id];
    let names: Vec<Option<Symbol>> = def
        .generics
        .lifetimes
        .iter()
        .map(|l| Some(l.name))
        .collect();
    let generics: Vec<Symbol> = def.generics.params.iter().map(|p| p.name).collect();
    let scope = TypeScope {
        generics: &generics,
        signature_of: Some("type aliases"),
        ..TypeScope::default()
    };
    let bounds = crate::traits::resolve_predicates(
        items,
        def.loc.module,
        &def.generics,
        scope,
        &mut Vec::new(),
    );
    let scope = TypeScope {
        bounds: &bounds.predicates,
        ..scope
    };
    let mut scratch = Vec::new();
    let ty = crate::resolve_type(
        items,
        def.loc.module,
        &def.ty,
        scope,
        def.ty_range,
        &mut scratch,
    );
    let mut walker = Walker::new(items, def.loc.module, names);
    walker.scope = scope;
    walker.body = body;
    walker.elided = Elided::Missing(None);
    let regions = walker.regions(&def.ty, ty);
    errors.append(&mut walker.errors);
    ACTIVE.with(|a| a.borrow_mut().pop());
    AliasRegions {
        ty: Some(ty),
        regions,
    }
}

/// Whether `name` is a lifetime parameter of a function whose body has
/// the block `module` is, or is in: which an item declared there cannot
/// use (E0401).
fn outer_lifetime(items: &ItemTree, module: ModuleId, name: Symbol) -> bool {
    let mut module = Some(module);
    while let Some(m) = module {
        let data = &items.modules[m];
        module = data.parent;
        let Some(f) = data.block.and_then(|b| b.function) else {
            continue;
        };
        let f = &items.functions[f];
        let owner = items.owner_generics(f.owner);
        if owner
            .into_iter()
            .chain([&f.generics])
            .any(|g| g.lifetimes.iter().any(|l| l.name == name))
        {
            return true;
        }
    }
    false
}

fn generic_lifetimes_in(args: &flux_hir::GenericArgs, out: &mut Vec<LifetimeRef>) {
    out.extend(args.lifetimes.iter().copied());
    for ty in &args.types {
        lifetimes_in(ty, out);
    }
    for binding in &args.bindings {
        if let Some(own) = &binding.args {
            generic_lifetimes_in(own, out);
        }
        lifetimes_in(&binding.ty, out);
    }
}

/// The lifetimes written in `ty`, into `out`.
pub(crate) fn lifetimes_in(ty: &TypeRef, out: &mut Vec<LifetimeRef>) {
    match ty {
        TypeRef::Ref {
            lifetime, pointee, ..
        } => {
            out.extend(lifetime.iter().copied());
            lifetimes_in(pointee, out);
        }
        TypeRef::Path(path) => {
            for args in path.args.iter().flatten() {
                generic_lifetimes_in(args, out);
            }
            if let Some(qself) = &path.qself {
                lifetimes_in(&qself.ty, out);
                if let Some(owner) = &qself.trait_ {
                    lifetimes_in(&TypeRef::Path(owner.clone()), out);
                }
            }
        }
        TypeRef::Tuple(list) => list.iter().for_each(|t| lifetimes_in(t, out)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => lifetimes_in(pointee, out),
        TypeRef::FnPtr {
            bound_lifetimes,
            params,
            ret,
            ..
        } => {
            let mut own = Vec::new();
            params.iter().for_each(|t| lifetimes_in(t, &mut own));
            lifetimes_in(ret, &mut own);
            out.extend(
                own.into_iter()
                    .filter(|l| !bound_lifetimes.contains(&l.name)),
            );
        }
        TypeRef::Dyn(d) => {
            for b in d.bounds.iter() {
                for args in b.path.args.iter().flatten() {
                    generic_lifetimes_in(args, out);
                }
            }
            out.extend(d.lifetime);
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

fn generic_elided_in(args: &flux_hir::GenericArgs, out: &mut Vec<TextRange>) {
    out.extend(
        args.lifetimes
            .iter()
            .filter(|l| l.is_anonymous())
            .map(|l| l.range),
    );
    for ty in &args.types {
        elided_in(ty, out);
    }
    for binding in &args.bindings {
        if let Some(own) = &binding.args {
            generic_elided_in(own, out);
        }
        elided_in(&binding.ty, out);
    }
}

/// Where `ty` elides a reference's lifetime (`&T` or `&'_ T`), into
/// `out`. Structs' elided lifetime arguments are not found: that needs
/// their definitions.
pub(crate) fn elided_in(ty: &TypeRef, out: &mut Vec<TextRange>) {
    match ty {
        TypeRef::Ref {
            lifetime,
            amp,
            pointee,
            ..
        } => {
            match lifetime {
                None => out.push(*amp),
                Some(l) if l.is_anonymous() => out.push(l.range),
                Some(_) => {}
            }
            elided_in(pointee, out);
        }
        TypeRef::Path(path) => {
            for args in path.args.iter().flatten() {
                generic_elided_in(args, out);
            }
        }
        TypeRef::Tuple(list) => list.iter().for_each(|t| elided_in(t, out)),
        TypeRef::Ptr { pointee, .. }
        | TypeRef::Array { elem: pointee, .. }
        | TypeRef::Slice(pointee) => elided_in(pointee, out),
        // `dyn Tr + '_`; without a bound, its default is not elided.
        TypeRef::Dyn(d) => {
            if let Some(l) = d.lifetime.filter(|l| l.is_anonymous()) {
                out.push(l.range);
            }
        }
        // A function pointer's own.
        TypeRef::FnPtr { .. }
        | TypeRef::Never
        | TypeRef::Infer
        | TypeRef::ImplTrait(_)
        | TypeRef::ImplParam(_)
        | TypeRef::Opaque(_)
        | TypeRef::Const(_)
        | TypeRef::Error => {}
    }
}

/// The help rustc gives for a return type's elided lifetime that
/// elision cannot find (E0106): `params` are the parameters' names (or
/// `None` for a pattern) and how many elided lifetimes each has, and
/// `named` whether the inputs have lifetimes that are not elided.
pub(crate) fn elision_help(params: &[(Option<Symbol>, usize)], named: bool) -> String {
    let start = "this function's return type contains a borrowed value";
    let name = |i: usize, n: Option<Symbol>| match n {
        Some(n) => format!("`{n}`"),
        None => format!("argument {}", i + 1),
    };
    let with: Vec<(usize, Option<Symbol>, usize)> = params
        .iter()
        .enumerate()
        .filter(|(_, (_, count))| *count > 0)
        .map(|(i, &(n, count))| (i, n, count))
        .collect();
    match with.as_slice() {
        [] if named => format!(
            "{start} with an elided lifetime, but the lifetime cannot be derived from the arguments"
        ),
        [] => format!("{start}, but there is no value for it to be borrowed from"),
        [(i, n, count)] => {
            let who = match n {
                Some(n) => format!("`{n}`'s"),
                None => format!("argument {}'s", i + 1),
            };
            format!(
                "{start}, but the signature does not say which one of {who} {count} lifetimes it is borrowed from"
            )
        }
        _ => {
            let names: Vec<String> = with.iter().map(|&(i, n, _)| name(i, n)).collect();
            let list = match names.as_slice() {
                [a, b] => format!("{a} or {b}"),
                [rest @ .., last] => format!("{}, or {last}", rest.join(", ")),
                [] => String::new(),
            };
            format!("{start}, but the signature does not say whether it is borrowed from {list}")
        }
    }
}

/// The lifetime parameters of `generics`, named.
pub(crate) fn own_names(generics: &Generics) -> impl Iterator<Item = LifetimeName> + '_ {
    generics.lifetimes.iter().map(|l| LifetimeName {
        name: Some(l.name),
        range: l.range,
    })
}

/// Requirements for a well-formed trait reference, in its caller's regions.
/// This instantiates the declaration's bounds (including `Self` at type slot
/// zero) and checks the types supplied as arguments. These are obligations,
/// never assumptions: a bound `N: Tr<'a, &'b T>` cannot imply `'b: 'a`.
/// Malformed region tables return `None`, rather than dropping constraints.
pub fn trait_requirements(
    items: &ItemTree,
    sigs: &Signatures,
    predicate: &crate::Predicate,
    self_regions: &[Region],
    regions: &TraitRefRegions,
) -> Option<Vec<Outlives<Region>>> {
    let trait_ref = predicate.trait_ref;
    let declaration = sigs.regions.traits.get(trait_ref.trait_)?;
    let types: Vec<Ty> = std::iter::once(predicate.self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    let tables: Vec<&[Region]> = std::iter::once(self_regions)
        .chain(regions.args.iter().map(|a| &**a))
        .collect();
    if regions.lifetimes.len() != declaration.names.len()
        || tables.len() != types.len()
        || types
            .iter()
            .zip(&tables)
            .any(|(&t, r)| slots(items, t) != r.len())
    {
        return None;
    }
    let map = |r| match r {
        Region::Param(i) => regions
            .lifetimes
            .get(i as usize)
            .copied()
            .unwrap_or(Region::Erased),
        r => r,
    };
    let mut out = Vec::new();
    for (&ty, table) in types.iter().zip(&tables) {
        implied_outlives_with_static(items, sigs, ty, table, Some(Region::Static), &mut out);
    }
    for bound in &declaration.outlives {
        match *bound {
            Outlives::Region(a, b) => out.push(Outlives::Region(map(a), map(b))),
            Outlives::Type(ty, b) => {
                let b = map(b);
                let mut table = Vec::new();
                instantiate(
                    items,
                    ty,
                    &[],
                    &mut |r| map(r),
                    &|i| tables.get(i as usize).map_or(Vec::new(), |r| r.to_vec()),
                    &mut table,
                );
                out.extend(table.into_iter().map(|a| Outlives::Region(a, b)));
                // Rust decides nominal outlives bounds from the type's
                // arguments. A projection used only in a field is not an
                // extra obligation on the impl header.
                for component in type_outlives_components(items, sigs, ty.subst(&types), false) {
                    out.push(Outlives::Type(component, b));
                }
            }
        }
    }
    out.retain(|p| !matches!(p, Outlives::Region(a, b) if a == b));
    Some(out)
}

/// What a well-formed `ty`, with slots `regions`, implies its lifetimes
/// outlive: `&'a T` that `T: 'a`, and a struct's inferred requirements.
pub fn implied_outlives<R: Copy + PartialEq>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    regions: &[R],
    out: &mut Vec<Outlives<R>>,
) {
    implied_outlives_with_static(items, sigs, ty, regions, None, out);
}

/// As [`implied_outlives`], also mapping explicit `'static` bounds of
/// ADTs and trait objects to `static_region` when checking well-formedness.
pub fn implied_outlives_with_static<R: Copy + PartialEq>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    regions: &[R],
    static_region: Option<R>,
    out: &mut Vec<Outlives<R>>,
) {
    let mut next = 0;
    implied_at(items, sigs, ty, regions, &mut next, static_region, out);
}

/// Projection rows describe selection inputs, not references stored in the
/// selected output. Its type-outlives requirement is kept separately.
fn stored_regions<R: Copy>(items: &ItemTree, ty: Ty, regions: &[R]) -> Vec<R> {
    fn visit<R: Copy>(items: &ItemTree, ty: Ty, row: &[R], next: &mut usize, out: &mut Vec<R>) {
        let own = match ty.kind() {
            TyKind::Alias(_) => {
                *next += slots(items, ty);
                return;
            }
            TyKind::Ref(..) => 1,
            TyKind::Dynamic(object) => object_own_slots(items, object),
            TyKind::Adt(adt, _) => items.structs[adt.strukt].generics.lifetimes.len(),
            TyKind::Opaque(opaque) => opaque.lifetimes as usize,
            TyKind::FnPtr(sig) => sig.regions.map_or(0, |r| r.free as usize),
            _ => 0,
        };
        out.extend_from_slice(&row[(*next).min(row.len())..(*next + own).min(row.len())]);
        *next += own;
        match ty.kind() {
            TyKind::Closure(_, args) => {
                visit(items, ClosureArgs::split(args).upvars, row, next, out);
            }
            TyKind::FnPtr(sig) => {
                if let Some(regions) = sig.regions {
                    for &ty in regions.types {
                        visit(items, ty, row, next, out);
                    }
                }
            }
            TyKind::Dynamic(object) => {
                for ty in object_region_types(items, object) {
                    visit(items, ty, row, next, out);
                }
            }
            TyKind::FnDef(def) => {
                *next += def.owner_lifetimes as usize;
                for &arg in def.args {
                    visit(items, arg, row, next, out);
                }
            }
            TyKind::Array(elem, _) => visit(items, elem, row, next, out),
            _ => {
                for child in ty.children() {
                    visit(items, child, row, next, out);
                }
            }
        }
    }
    let mut out = Vec::new();
    visit(items, ty, regions, &mut 0, &mut out);
    out
}

fn implied_at<R: Copy + PartialEq>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    regions: &[R],
    next: &mut usize,
    static_region: Option<R>,
    out: &mut Vec<Outlives<R>>,
) {
    match ty.kind() {
        TyKind::Ref(_, inner) => {
            let Some(&r) = regions.get(*next) else { return };
            *next += 1;
            let start = *next;
            implied_at(items, sigs, inner, regions, next, static_region, out);
            // Everything in the pointee outlives the reference.
            for inner_r in stored_regions(items, inner, &regions[start..*next]) {
                if inner_r != r {
                    out.push(Outlives::Region(inner_r, r));
                }
            }
            outlives_params(items, sigs, inner, r, out);
        }
        TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            implied_at(items, sigs, t, regions, next, static_region, out)
        }
        TyKind::Tuple(list) => {
            for &t in list {
                implied_at(items, sigs, t, regions, next, static_region, out);
            }
        }
        TyKind::Alias(alias) if has_projection_inputs(items, alias) => {
            let owner = *next;
            *next += alias.owner_lifetimes as usize;
            let mut arg_ranges = Vec::new();
            for &ty in alias.args {
                let start = *next;
                implied_at(items, sigs, ty, regions, next, static_region, out);
                arg_ranges.push((start, *next));
            }
            let family = *next;
            *next += alias.family_lifetimes as usize;
            for &ty in alias.family_args {
                let start = *next;
                implied_at(items, sigs, ty, regions, next, static_region, out);
                arg_ranges.push((start, *next));
            }
            // A selected projection must prove its applicability before it
            // normalizes. Only an abstract family contributes implied facts;
            // its eventual caller must establish those prerequisites.
            if flux_solve::traits::select_impl(
                items,
                sigs,
                crate::Predicate {
                    self_ty: alias.args[0],
                    trait_ref: crate::TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: &alias.args[1..],
                    },
                },
            )
            .is_some()
            {
                return;
            }
            let Some(decl) = sigs.regions.traits.get(alias.trait_).and_then(|t| {
                t.families
                    .iter()
                    .find(|(name, _)| *name == alias.name)
                    .map(|(_, f)| f)
            }) else {
                return;
            };
            let lifetime = |r: Region| match r {
                Region::Static => static_region,
                Region::Param(i) if i < alias.owner_lifetimes => {
                    regions.get(owner + i as usize).copied()
                }
                Region::Param(i) => regions
                    .get(family + (i - alias.owner_lifetimes) as usize)
                    .copied(),
                _ => None,
            };
            let args = alias.template_args();
            // A projection in a well-formed signature carries its family's
            // prerequisites just as an ADT carries its declared field bounds.
            for prerequisite in &decl.outlives {
                match *prerequisite {
                    Outlives::Region(a, b) => {
                        if let (Some(a), Some(b)) = (lifetime(a), lifetime(b))
                            && a != b
                        {
                            out.push(Outlives::Region(a, b));
                        }
                    }
                    Outlives::Type(ty, b) => {
                        let Some(b) = lifetime(b) else { continue };
                        if let TyKind::Param(param) = ty.kind()
                            && let Some(&(start, end)) = arg_ranges.get(param.index as usize)
                        {
                            for a in stored_regions(
                                items,
                                args[param.index as usize],
                                &regions[start.min(regions.len())..end.min(regions.len())],
                            ) {
                                if a != b {
                                    out.push(Outlives::Region(a, b));
                                }
                            }
                        }
                        outlives_params(items, sigs, ty.subst(&args), b, out);
                    }
                }
            }
        }
        TyKind::Adt(adt, args) => {
            let lifetimes = items.structs[adt.strukt].generics.lifetimes.len();
            let start = *next;
            *next += lifetimes;
            let mut arg_ranges = Vec::new();
            for &t in args {
                let from = *next;
                implied_at(items, sigs, t, regions, next, static_region, out);
                arg_ranges.push((from, *next));
            }
            let Some(def) = sigs.regions.structs.get(adt.strukt) else {
                return;
            };
            let lifetime = |r: Region| match r {
                Region::Param(i) => regions.get(start + i as usize).copied(),
                Region::Static => static_region,
                _ => None,
            };
            for p in def.outlives.iter() {
                match *p {
                    Outlives::Region(a, b) => {
                        if let (Some(a), Some(b)) = (lifetime(a), lifetime(b))
                            && a != b
                        {
                            out.push(Outlives::Region(a, b));
                        }
                    }
                    Outlives::Type(t, b) => {
                        let Some(b) = lifetime(b) else { continue };
                        let TyKind::Param(p) = t.kind() else {
                            // A well-formed ADT also carries declared and
                            // field-implied projection contracts, e.g.
                            // `I::Item: 'a`. Substitute the complete type;
                            // requiring only its inputs to outlive would
                            // strengthen or lose the associated-value bound.
                            outlives_params(items, sigs, t.subst(args), b, out);
                            continue;
                        };
                        let Some(&arg) = args.get(p.index as usize) else {
                            continue;
                        };
                        let (from, to) = arg_ranges[p.index as usize];
                        for a in stored_regions(
                            items,
                            arg,
                            &regions[from.min(regions.len())..to.min(regions.len())],
                        ) {
                            if a != b {
                                out.push(Outlives::Region(a, b));
                            }
                        }
                        outlives_params(items, sigs, arg, b, out);
                    }
                }
            }
        }
        TyKind::Closure(_, args) => implied_at(
            items,
            sigs,
            ClosureArgs::split(args).upvars,
            regions,
            next,
            static_region,
            out,
        ),
        TyKind::Opaque(opaque) => {
            *next += opaque.lifetimes as usize;
            for &t in opaque.args {
                implied_at(items, sigs, t, regions, next, static_region, out);
            }
        }
        TyKind::Dynamic(object) => {
            let bound = regions.get(*next).copied();
            *next += object_own_slots(items, object);
            // Well-formed occurrences of an object inherit a principal's
            // static Self contract. This is an occurrence obligation/assumption,
            // not a property of its erased Ty or of a free lifetime by itself.
            if let (Some(bound), Some(static_region)) = (bound, static_region)
                && sigs
                    .regions
                    .traits
                    .get(object.trait_)
                    .is_some_and(|declaration| {
                        declaration.outlives.iter().any(|requirement| {
                            matches!(requirement,
                        Outlives::Type(ty, Region::Static)
                            if matches!(ty.kind(), TyKind::Param(p) if p.index == 0))
                        })
                    })
                && bound != static_region
            {
                out.push(Outlives::Region(bound, static_region));
            }
            for ty in object_region_types(items, object) {
                implied_at(items, sigs, ty, regions, next, static_region, out);
            }
        }
        TyKind::FnPtr(sig) => {
            if let Some(r) = sig.regions {
                *next += r.free as usize;
                for &ty in r.types {
                    implied_at(items, sigs, ty, regions, next, static_region, out);
                }
            }
        }
        _ => {}
    }
}

/// `T: r` for each parameter and associated type in `ty`.
fn outlives_params<R: Copy>(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    r: R,
    out: &mut Vec<Outlives<R>>,
) {
    for component in outlives_type_components(items, sigs, ty) {
        out.push(Outlives::Type(component, r));
    }
}

/// Declarations whose stored fields can expose an associated-type projection.
/// Generic arguments supply their own requirements independently of this set.
pub fn outlives_structs_with_projections(
    sigs: &Signatures,
    ty: Ty,
) -> rustc_hash::FxHashSet<flux_hir::StructId> {
    // Generic arguments already account for every stored type parameter.
    // Field expansion adds information only when it can expose a projection.
    // Find that nominal closure without instantiating recursive declarations.
    fn dependencies(ty: Ty, adts: &mut Vec<flux_hir::StructId>, projection: &mut bool) {
        match ty.kind() {
            TyKind::Alias(_) => *projection = true,
            TyKind::FnDef(def) => {
                for &arg in def.args {
                    dependencies(arg, adts, projection);
                }
            }
            TyKind::FnPtr(sig) => {
                if let Some(r) = sig.regions {
                    for &ty in r.types {
                        dependencies(ty, adts, projection);
                    }
                }
            }
            TyKind::Closure(_, args) => {
                dependencies(ClosureArgs::split(args).upvars, adts, projection)
            }
            TyKind::Adt(adt, args) => {
                adts.push(adt.strukt);
                for &arg in args {
                    dependencies(arg, adts, projection);
                }
            }
            _ => {
                for child in ty.children() {
                    dependencies(child, adts, projection);
                }
            }
        }
    }
    let mut projected = rustc_hash::FxHashSet::default();
    let mut parents: rustc_hash::FxHashMap<flux_hir::StructId, Vec<flux_hir::StructId>> =
        Default::default();
    let mut pending = Vec::new();
    let mut reachable = Vec::new();
    dependencies(ty, &mut reachable, &mut false);
    let mut visited = rustc_hash::FxHashSet::default();
    while let Some(id) = reachable.pop() {
        if !visited.insert(id) {
            continue;
        }
        let Some(fields) = sigs.structs.get(id) else {
            continue;
        };
        let mut children = Vec::new();
        let mut projection = false;
        for &field in *fields {
            dependencies(field, &mut children, &mut projection);
        }
        if projection && projected.insert(id) {
            pending.push(id);
        }
        for child in children {
            reachable.push(child);
            parents.entry(child).or_default().push(id);
        }
    }
    while let Some(id) = pending.pop() {
        if let Some(parents) = parents.get(&id) {
            for &parent in parents {
                if projected.insert(parent) {
                    pending.push(parent);
                }
            }
        }
    }
    projected
}

/// Abstract types whose stored values must outlive a reference to `ty`.
/// ADT fields can contain projections absent from their generic arguments,
/// such as `Cow<B>`'s `B::Owned`. Include those components in both implied
/// assumptions and obligations, using the same storage traversal.
pub fn outlives_type_components(items: &ItemTree, sigs: &Signatures, ty: Ty) -> Vec<Ty> {
    type_outlives_components(items, sigs, ty, true)
}

fn type_outlives_components(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    include_projected_fields: bool,
) -> Vec<Ty> {
    // A nominal type gets its free regions through its declaration inputs.
    // A closed instance has no such regions. Its hidden projections can have
    // explicit lifetime rows, which cannot be recovered from their erased types.
    if is_closed_nominal(items, ty) {
        return Vec::new();
    }
    // Leaf queries cannot expose nominal fields. Hidden-field traversal issues
    // many of these; do not rebuild the complete declaration graph for each one.
    match ty.kind() {
        TyKind::Param(_) | TyKind::Alias(_) => return vec![ty],
        TyKind::Int(_)
        | TyKind::Float(_)
        | TyKind::Bool
        | TyKind::Char
        | TyKind::Str
        | TyKind::Never
        | TyKind::CStr
        | TyKind::Infer(_)
        | TyKind::Value(_)
        | TyKind::Error
        | TyKind::Tuple([]) => return Vec::new(),
        _ => {}
    }
    type Cutoffs = Vec<flux_hir::StructId>;

    struct Components<'a> {
        items: &'a ItemTree,
        sigs: &'a Signatures,
        projected: rustc_hash::FxHashSet<flux_hir::StructId>,
        active: Vec<flux_hir::StructId>,
        expanding: rustc_hash::FxHashSet<(Ty, usize)>,
        visited: rustc_hash::FxHashMap<Ty, Vec<Cutoffs>>,
        out: Vec<Ty>,
    }

    impl Components<'_> {
        /// The ancestor declarations that stopped this walk. Other ancestors
        /// did not affect what it could discover.
        fn visit(&mut self, ty: Ty) -> Cutoffs {
            if let Some(covered) = self.visited.get(&ty).and_then(|contexts| {
                contexts
                    .iter()
                    .find(|cutoffs| cutoffs.iter().all(|id| self.active.contains(id)))
            }) {
                return covered.clone();
            }
            // At the same depth a nested walk has the same active prefix.
            // This also terminates a recursive coroutine witness without
            // treating the unfinished walk as context-independent.
            let path = (ty, self.active.len());
            if !self.expanding.insert(path) {
                return self.active.clone();
            }
            let blocked = self.walk(ty);
            self.expanding.remove(&path);
            let contexts = self.visited.entry(ty).or_default();
            contexts.retain(|old| !blocked.iter().all(|id| old.contains(id)));
            contexts.push(blocked.clone());
            blocked
        }

        fn extend(cutoffs: &mut Cutoffs, additional: Cutoffs) {
            for id in additional {
                if !cutoffs.contains(&id) {
                    cutoffs.push(id);
                }
            }
        }

        fn walk(&mut self, ty: Ty) -> Cutoffs {
            let mut blocked = Vec::new();
            let items = self.items;
            let sigs = self.sigs;
            match ty.kind() {
                TyKind::Param(_) | TyKind::Alias(_) => {
                    if !self.out.contains(&ty) {
                        self.out.push(ty);
                    }
                }
                TyKind::FnDef(def) => {
                    // Even a zero-sized function item captures its substitutions.
                    for &arg in def.args {
                        Self::extend(&mut blocked, self.visit(arg));
                    }
                }
                TyKind::FnPtr(sig) => {
                    if let Some(r) = sig.regions {
                        for &ty in r.types {
                            Self::extend(&mut blocked, self.visit(ty));
                        }
                    }
                }
                // A closure stores its captures. An array stores its elements.
                TyKind::Closure(_, args) => return self.visit(ClosureArgs::split(args).upvars),
                TyKind::Array(elem, _) => return self.visit(elem),
                TyKind::Adt(adt, args)
                    if flux_solve::is_lang_struct(items, adt.strukt, "coroutine")
                        && let Some(&closure_ty) = args.first()
                        && let TyKind::Closure(id, closure_args) = closure_ty.kind()
                        && let Some(stored) = sigs.coroutine_witnesses.get(&id)
                        && stored.iter().all(|ty| {
                            slots(items, ty.subst(ClosureArgs::split(closure_args).parent)) == 0
                        }) =>
                {
                    // The captures and suspension storage hold the future's
                    // values. Its output type is only a marker.
                    let closure = ClosureArgs::split(closure_args);
                    blocked = self.visit(closure.upvars);
                    for &ty in *stored {
                        Self::extend(&mut blocked, self.visit(ty.subst(closure.parent)));
                    }
                }
                TyKind::Adt(..) if is_closed_nominal(items, ty) => {}
                TyKind::Adt(adt, args) => {
                    // Symbolic const arguments are also represented by Param.
                    for (&arg, parameter) in
                        args.iter().zip(&items.structs[adt.strukt].generics.params)
                    {
                        if !parameter.is_const() {
                            Self::extend(&mut blocked, self.visit(arg));
                        }
                    }
                    if self.projected.contains(&adt.strukt)
                        && let Some(fields) = sigs.structs.get(adt.strukt)
                    {
                        if self.active.contains(&adt.strukt) {
                            Self::extend(&mut blocked, vec![adt.strukt]);
                        } else {
                            self.active.push(adt.strukt);
                            for &field in *fields {
                                let mut field_blocked = self.visit(field.subst(args));
                                // This declaration is active in its own fields
                                // every time, independently of the outer path.
                                field_blocked.retain(|&id| id != adt.strukt);
                                Self::extend(&mut blocked, field_blocked);
                            }
                            self.active.pop();
                        }
                    }
                }
                _ => {
                    if ty.has_params() || ty.has_aliases() {
                        for child in ty.children() {
                            Self::extend(&mut blocked, self.visit(child));
                        }
                    }
                }
            }
            blocked
        }
    }

    let mut components = Components {
        items,
        sigs,
        projected: if include_projected_fields {
            outlives_structs_with_projections(sigs, ty)
        } else {
            Default::default()
        },
        active: Vec::new(),
        expanding: Default::default(),
        visited: Default::default(),
        out: Vec::new(),
    };
    components.visit(ty);
    components.out
}

/// Admit only declarations whose complete required-method representation can be
/// reconstructed. Obligation checking and unsupported use-site guards remain
/// independent of this declaration boundary.
pub(crate) fn check_trait_admission(
    items: &ItemTree,
    sigs: &crate::Signatures,
    from: flux_hir::ItemCounts,
    diagnostics: &mut Vec<flux_hir::Diagnostic>,
) {
    for (id, t) in items.traits.iter().skip(from.traits as usize) {
        if t.generics.lifetimes.is_empty()
            || trait_profile::required_method_profile(items, diagnostics, sigs, id).is_ok()
            || trait_profile::lifetime_marker_profile(items, diagnostics, sigs, id)
            || trait_profile::family_method_profile(items, diagnostics, sigs, id)
            || trait_profile::safe_method_profile(items, diagnostics, sigs, id).is_ok()
            || trait_profile::generic_method_profile(items, diagnostics, sigs, id).is_ok()
        {
            continue;
        }
        if std::env::var_os("FLUX_TRACE_TRAIT_PROFILE").is_some() {
            eprintln!(
                "trait {}: {:?} {:?}",
                t.name,
                trait_profile::safe_method_profile(items, diagnostics, sigs, id),
                trait_profile::generic_method_profile(items, diagnostics, sigs, id)
            );
        }
        for l in &t.generics.lifetimes {
            diagnostics.push(flux_hir::Diagnostic {
                file: t.loc.file,
                range: l.range,
                message: "lifetime parameters on traits are not supported yet".to_owned(),
            });
        }
    }
}

/// Explicit calls can fix these free-function binders: every named lifetime
/// has a written outlives bound, and the signature adds no elided binders.
pub fn named_call_lifetimes(
    items: &ItemTree,
    sigs: &Signatures,
    function: flux_hir::FunctionId,
) -> Option<u32> {
    let data = &items.functions[function];
    let regions = sigs.regions.fns.get(function)?;
    let names = &data.generics.lifetimes;
    if data.owner.is_some()
        || regions.owner != 0
        || regions.names.len() != names.len()
        || names.iter().zip(&regions.names).any(|(written, stored)| {
            stored.name != Some(written.name) || stored.range != written.range
        })
        || names.iter().any(|name| {
            !data
                .generics
                .outlives
                .iter()
                .any(|bound| bound.longer.name == name.name || bound.shorter.name == name.name)
        })
    {
        return None;
    }
    Some(names.len() as u32)
}
