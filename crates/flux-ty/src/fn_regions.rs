//! Canonical region recipes for bare function-pointer types.
use crate::{FnPtrRegions, FnRegion, Ty, TyKind};
use flux_hir::{ItemTree, LifetimeRef, ModuleId, TypeRef};
use flux_intern::Symbol;
use flux_syntax::TextRange;

pub(crate) struct Description {
    pub regions: &'static FnPtrRegions,
    pub free: Vec<LifetimeRef>,
    pub captures: Vec<(TypeRef, Ty)>,
}

struct Builder<'a> {
    items: &'a ItemTree,
    module: ModuleId,
    names: Vec<(Symbol, Option<u32>)>,
    bound: u32,
    free: Vec<LifetimeRef>,
    captures: Vec<(TypeRef, Ty)>,
    projections: Vec<(crate::AliasTy, Vec<FnRegion>)>,
    inputs: Vec<FnRegion>,
    output: bool,
    enclosing: Option<FnRegion>,
    elision_only: bool,
    errors: &'a mut Vec<(TextRange, String)>,
}

pub(crate) fn describe(
    items: &ItemTree,
    module: ModuleId,
    names: &[Symbol],
    params: &[TypeRef],
    ret: &TypeRef,
    signature: crate::FnSigTy,
    errors: &mut Vec<(TextRange, String)>,
) -> Description {
    for (i, name) in names.iter().enumerate() {
        if names[..i].contains(name) {
            errors.push((
                TextRange::default(),
                "lifetime name is already in use".into(),
            ));
        }
    }
    let mut cx = Builder {
        items,
        module,
        names: names.iter().map(|&n| (n, None)).collect(),
        bound: 0,
        free: Vec::new(),
        captures: Vec::new(),
        projections: Vec::new(),
        inputs: Vec::new(),
        output: false,
        enclosing: None,
        elision_only: false,
        errors,
    };
    let mut rows = Vec::new();
    for (written, &ty) in params.iter().zip(signature.params()) {
        let candidates = cx.elision_regions(written, ty);
        let row = cx.walk(written, ty);
        for &region in &candidates {
            if !matches!(region, FnRegion::Type(_) | FnRegion::Projection(_))
                && !cx.inputs.contains(&region)
            {
                cx.inputs.push(region);
            }
        }
        rows.push(row);
    }
    cx.output = true;
    rows.push(cx.walk(ret, signature.ret()));
    fn unconstrained(
        region: FnRegion,
        projections: &[(crate::AliasTy, Vec<FnRegion>)],
        inputs: &[FnRegion],
        depth: u32,
    ) -> bool {
        if depth > 64 {
            return true;
        }
        match region {
            FnRegion::Bound(_) => !inputs.contains(&region),
            FnRegion::Projection(i) => projections.get(i as usize).is_none_or(|(_, row)| {
                row.iter()
                    .any(|&r| unconstrained(r, projections, inputs, depth + 1))
            }),
            _ => false,
        }
    }
    if rows.last().is_some_and(|row| {
        row.iter()
            .any(|&r| unconstrained(r, &cx.projections, &cx.inputs, 0))
    }) {
        cx.errors.push((
            TextRange::default(),
            "return lifetime is not constrained by the function inputs".into(),
        ));
    }
    let types: Vec<_> = cx.captures.iter().map(|(_, ty)| *ty).collect();
    let regions = FnPtrRegions::intern_with_projections(
        cx.bound,
        cx.free.len() as u32,
        &rows,
        &types,
        &cx.projections,
    );
    Description {
        regions,
        free: cx.free,
        captures: cx.captures,
    }
}

impl Builder<'_> {
    fn lifetime(&mut self, lifetime: Option<LifetimeRef>, at: TextRange) -> FnRegion {
        if let Some(lifetime) = lifetime.filter(|l| !l.is_anonymous()) {
            if lifetime.is_static() {
                return FnRegion::Static;
            }
            if let Some(index) = self
                .names
                .iter()
                .position(|(name, _)| *name == lifetime.name)
            {
                let bound = match self.names[index].1 {
                    Some(bound) => bound,
                    None => {
                        let next = self.bound;
                        self.bound += 1;
                        self.names[index].1 = Some(next);
                        next
                    }
                };
                return FnRegion::Bound(bound);
            }
            let index = match self.free.iter().position(|l| l.name == lifetime.name) {
                Some(i) => i,
                None => {
                    self.free.push(lifetime);
                    self.free.len() - 1
                }
            };
            return FnRegion::Free(index as u32);
        }
        if self.output {
            if let [region] = &*self.inputs {
                return *region;
            }
            self.errors.push((at, "missing lifetime specifier".into()));
            // The error prevents admission; this placeholder is never a proof.
            return FnRegion::Static;
        }
        let next = self.bound;
        self.bound += 1;
        FnRegion::Bound(next)
    }

    /// The recipe of `ty`, written as alias `id` with the arguments of
    /// `path`: the alias's own region table with its lifetimes as written
    /// here (an elided one is one lifetime, as rustc has it) and its type
    /// parameters' recipes as their arguments'. `None` for an alias whose
    /// table or arguments this cannot map.
    fn alias(
        &mut self,
        path: &flux_hir::Path,
        id: flux_hir::TypeAliasId,
        ty: Ty,
    ) -> Option<Vec<FnRegion>> {
        use crate::regions::Region;
        let def = &self.items.type_aliases[id];
        let want = def.generics.lifetimes.len();
        let mut errors = Vec::new();
        let table = crate::regions::alias_regions(self.items, id, &mut errors);
        let alias_ty = table.ty?;
        if !errors.is_empty()
            || table.regions.iter().any(|r| match *r {
                Region::Param(i) => i as usize >= want,
                Region::Static => false,
                Region::Erased | Region::Late(_) => true,
            })
        {
            return None;
        }
        let last = path.segments.len().saturating_sub(1);
        let args = path.args(last);
        let written: &[LifetimeRef] = args.map_or(&[], |a| &a.lifetimes);
        if !written.is_empty() && written.len() != want {
            return None;
        }
        let at = path.range(last, TextRange::default());
        let lifetimes: Vec<FnRegion> = (0..want)
            .map(|i| self.lifetime(written.get(i).copied(), at))
            .collect();
        // The instance's type arguments, matched against the alias's type.
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
        let mut types = Vec::new();
        for i in 0..def.generics.params.len() {
            let Some(arg) = resolved.get(i).copied().flatten() else {
                types.push(Vec::new());
                continue;
            };
            match args.and_then(|a| a.types.get(i)) {
                Some(written) => types.push(self.walk(written, arg)),
                // A default's lifetimes are its declaration's.
                None if crate::regions::slots(self.items, arg) == 0
                    && !arg.has_params()
                    && !arg.has_infer()
                    && !arg.has_aliases() =>
                {
                    types.push(Vec::new())
                }
                None => return None,
            }
        }
        let mut map = |r: Region| match r {
            Region::Param(i) => lifetimes[i as usize],
            _ => FnRegion::Static,
        };
        let args = |j: u32| types.get(j as usize).cloned().unwrap_or_default();
        let mut out = Vec::new();
        crate::regions::instantiate(
            self.items,
            alias_ty,
            &table.regions,
            &mut map,
            &args,
            &mut out,
        );
        Some(out)
    }

    fn projection(
        &mut self,
        path: &flux_hir::Path,
        alias: crate::AliasTy,
    ) -> Option<Vec<FnRegion>> {
        let qself = path.qself.as_ref()?;
        let owner = qself.trait_.as_ref()?;
        if self
            .items
            .resolve_trait(
                self.module,
                &flux_hir::Bound {
                    path: owner.clone(),
                    bound_lifetimes: Box::default(),
                    range: TextRange::default(),
                },
            )
            .0
            != Some(alias.trait_)
        {
            return None;
        }
        let owner_args = owner.args(owner.segments.len().saturating_sub(1));
        let lifetimes = owner_args.map_or(&[][..], |a| &a.lifetimes);
        if lifetimes.len() != alias.owner_lifetimes as usize {
            return None;
        }
        let mut inputs = Vec::new();
        for &lifetime in lifetimes {
            inputs.push(self.lifetime(Some(lifetime), lifetime.range));
        }
        inputs.extend(self.walk(&qself.ty, alias.args[0]));
        for (index, &ty) in alias.args.iter().skip(1).enumerate() {
            if let Some(written) = owner_args.and_then(|a| a.types.get(index)) {
                inputs.extend(self.walk(written, ty));
            } else if ty.has_params()
                || ty.has_aliases()
                || crate::regions::slots(self.items, ty) != 0
            {
                return None;
            }
        }
        let family_args = path.args(path.segments.len().saturating_sub(1));
        let lifetimes = family_args.map_or(&[][..], |a| &a.lifetimes);
        if lifetimes.len() != alias.family_lifetimes as usize {
            return None;
        }
        for &lifetime in lifetimes {
            inputs.push(self.lifetime(Some(lifetime), lifetime.range));
        }
        for (index, &ty) in alias.family_args.iter().enumerate() {
            inputs.extend(self.walk(family_args?.types.get(index)?, ty));
        }
        let index = match self
            .projections
            .iter()
            .position(|(old, row)| *old == alias && *row == inputs)
        {
            Some(index) => index,
            None => {
                let index = self.projections.len();
                self.projections.push((alias, inputs));
                index
            }
        };
        Some(vec![FnRegion::Projection(index as u32)])
    }

    fn capture(&mut self, written: &TypeRef, ty: Ty) -> Vec<FnRegion> {
        if !ty.has_params()
            && !ty.has_infer()
            && !ty.has_aliases()
            && crate::regions::slots(self.items, ty) == 0
        {
            return Vec::new();
        }
        // Repeated occurrences of one type parameter share its stored regions.
        // Distinct parameters must remain separate even after substitution.
        if matches!(ty.kind(), TyKind::Param(_))
            && let Some(index) = self
                .captures
                .iter()
                .position(|(_, captured)| *captured == ty)
        {
            return vec![FnRegion::Type(index as u32)];
        }
        let index = self.captures.len();
        self.captures.push((written.clone(), ty));
        vec![FnRegion::Type(index as u32)]
    }

    fn elision_regions(&self, written: &TypeRef, ty: Ty) -> Vec<FnRegion> {
        let mut errors = Vec::new();
        let mut view = Builder {
            items: self.items,
            module: self.module,
            names: self.names.clone(),
            bound: self.bound,
            free: self.free.clone(),
            captures: self.captures.clone(),
            projections: self.projections.clone(),
            inputs: self.inputs.clone(),
            output: self.output,
            enclosing: self.enclosing,
            elision_only: true,
            errors: &mut errors,
        };
        view.walk(written, ty)
    }

    fn walk(&mut self, written: &TypeRef, ty: Ty) -> Vec<FnRegion> {
        // Only an immediately enclosing reference supplies the default object
        // lifetime. Containers establish a new defaulting context.
        let enclosing = self.enclosing.take();
        if let TypeRef::Path(path) = written
            && let Ok(flux_hir::def_map::Resolved::Def(flux_hir::def_map::Def::TypeAlias(id))) =
                self.items
                    .resolve(self.module, path, flux_hir::def_map::Ns::Types)
        {
            if crate::regions::slots(self.items, ty) == 0 {
                return self.capture(written, ty);
            }
            if let Some(row) = self.alias(path, id, ty) {
                return row;
            }
            self.errors.push((
                path.range(path.segments.len().saturating_sub(1), TextRange::default()),
                "region-bearing aliases inside function pointers are not supported yet".into(),
            ));
            return self.capture(written, ty);
        }
        match (written, ty.kind()) {
            (
                TypeRef::Ref {
                    lifetime,
                    amp,
                    pointee,
                    ..
                },
                TyKind::Ref(_, inner),
            ) => {
                let region = self.lifetime(*lifetime, *amp);
                let previous = self.enclosing.replace(region);
                let mut row = vec![region];
                row.extend(self.walk(pointee, inner));
                self.enclosing = previous;
                row
            }
            (TypeRef::Dyn(d), TyKind::Dynamic(dynamic)) => {
                for bound in &d.bounds {
                    let (trait_, _) = self.items.resolve_trait(self.module, bound);
                    if trait_.is_some_and(|id| !self.items.traits[id].generics.lifetimes.is_empty())
                        || bound
                            .path
                            .args
                            .iter()
                            .flatten()
                            .any(|a| !a.lifetimes.is_empty())
                    {
                        self.errors.push((
                            bound.range,
                            "lifetime arguments on trait objects are not supported yet".into(),
                        ));
                    }
                }
                let region = match d.lifetime {
                    Some(l) => Some(self.lifetime(Some(l), l.range)),
                    None if self.elision_only => None,
                    None if crate::regions::object_has_static_self(self.items, dynamic.trait_) => {
                        Some(FnRegion::Static)
                    }
                    None => Some(enclosing.unwrap_or(FnRegion::Static)),
                };
                let mut row: Vec<_> = region.into_iter().collect();
                if crate::fn_trait_kind(self.items, dynamic.trait_).is_some() {
                    if dynamic
                        .args
                        .iter()
                        .chain(dynamic.assoc_tys)
                        .any(|&ty| ty.has_params() || crate::regions::slots(self.items, ty) != 0)
                    {
                        self.errors.push((TextRange::default(),
                            "callable trait object argument regions inside function pointers are not supported yet".into()));
                    }
                    return row;
                }
                // Match the ordinary object region layout: the object bound,
                // principal arguments, then equalities in declaring-trait order.
                if let Some(bound) = d.bounds.iter().find(|bound| {
                    self.items.resolve_trait(self.module, bound).0 == Some(dynamic.trait_)
                }) {
                    let args = bound.path.args(bound.path.segments.len().saturating_sub(1));
                    for (index, &ty) in dynamic.args.iter().enumerate() {
                        let written = args.and_then(|args| args.types.get(index));
                        row.extend(self.object_argument(written, ty));
                    }
                    for (&name, &ty) in dynamic.assoc_names.iter().zip(dynamic.assoc_tys) {
                        let written = args
                            .and_then(|args| {
                                args.bindings.iter().find(|binding| binding.name == name)
                            })
                            .map(|binding| &binding.ty);
                        row.extend(self.object_argument(written, ty));
                    }
                }
                row
            }
            (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(_, inner)) => self.walk(pointee, inner),
            (TypeRef::Array { elem, .. }, TyKind::Array(inner, _))
            | (TypeRef::Slice(elem), TyKind::Slice(inner)) => self.walk(elem, inner),
            (TypeRef::Tuple(written), TyKind::Tuple(types)) => {
                let mut row = Vec::new();
                for (written, &ty) in written.iter().zip(types) {
                    row.extend(self.walk(written, ty));
                }
                row
            }
            (TypeRef::Path(path), TyKind::Alias(alias))
                if crate::regions::has_projection_inputs(self.items, alias) =>
            {
                if let Some(row) = self.projection(path, alias) {
                    return row;
                }
                self.errors.push((
                    path.range(0, TextRange::default()),
                    "function pointer projection inputs are not represented".into(),
                ));
                Vec::new()
            }
            (TypeRef::Path(path), TyKind::Adt(adt, args)) => {
                let root = self.items.structs[adt.strukt].generics.lifetimes.len();
                let last = path.segments.len().saturating_sub(1);
                let written_args = path.args(last);
                let mut row = Vec::new();
                for i in 0..root {
                    let lifetime = written_args.and_then(|a| a.lifetimes.get(i)).copied();
                    row.push(self.lifetime(lifetime, path.range(last, TextRange::default())));
                }
                for (i, &arg) in args.iter().enumerate() {
                    if let Some(written) = written_args.and_then(|a| a.types.get(i)) {
                        row.extend(self.walk(written, arg));
                    } else {
                        // The default's source lifetime recipe is not the
                        // caller's. Region-free defaults need no such mapping.
                        if crate::regions::slots(self.items, arg) != 0 {
                            self.errors.push((path.range(last, TextRange::default()),
                                "region-bearing defaults inside function pointers are not supported yet".into()));
                        }
                        row.extend(self.capture(written, arg));
                    }
                }
                row
            }
            (TypeRef::FnPtr { .. }, TyKind::FnPtr(sig)) => {
                // Capture the nested pointer as one type group. Its own
                // recipe retains generic type regions after substitution.
                // Written free lifetimes still need binder remapping.
                if sig.regions.is_some_and(|r| r.free != 0) {
                    self.errors.push((
                        TextRange::default(),
                        "captured regions in nested function pointers are not supported yet".into(),
                    ));
                }
                self.capture(written, ty)
            }
            _ => self.capture(written, ty),
        }
    }

    fn object_argument(&mut self, written: Option<&TypeRef>, ty: Ty) -> Vec<FnRegion> {
        if let Some(written) = written {
            return self.walk(written, ty);
        }
        if ty.has_params()
            || ty.has_aliases()
            || ty.has_infer()
            || crate::regions::slots(self.items, ty) != 0
        {
            self.errors.push((TextRange::default(),
                "inherited trait object argument regions inside function pointers are not supported yet".into()));
        }
        Vec::new()
    }
}

/// Variance of the value's free regions and captured type groups. Bound
/// lifetimes are not value slots and must never consume an outer declaration's
/// region table while computing the variance of a field containing a pointer.
pub fn variances(
    items: &ItemTree,
    sigs: &crate::Signatures,
    sig: crate::FnSigTy,
) -> Option<(Vec<crate::regions::Variance>, Vec<crate::regions::Variance>)> {
    use crate::regions::Variance as V;
    struct Walk<'a> {
        items: &'a ItemTree,
        sigs: &'a crate::Signatures,
        r: &'a FnPtrRegions,
        free: Vec<V>,
        types: Vec<V>,
    }
    impl Walk<'_> {
        fn slot(&mut self, row: &[FnRegion], at: &mut usize, variance: V) -> Option<()> {
            let term = row.get(*at)?;
            *at += 1;
            match *term {
                FnRegion::Free(i) => {
                    let slot = self.free.get_mut(i as usize)?;
                    *slot = slot.join(variance);
                }
                FnRegion::Bound(_) | FnRegion::Static => {}
                FnRegion::Type(_) | FnRegion::Projection(_) => return None,
            }
            Some(())
        }
        fn projection(&mut self, index: u32, depth: u32) -> Option<()> {
            if depth > 64 {
                return None;
            }
            let inputs = self.r.projections.get(index as usize)?.inputs;
            for &input in inputs {
                match input {
                    FnRegion::Free(i) => *self.free.get_mut(i as usize)? = V::Invariant,
                    FnRegion::Type(i) => *self.types.get_mut(i as usize)? = V::Invariant,
                    FnRegion::Projection(i) => self.projection(i, depth + 1)?,
                    FnRegion::Bound(_) | FnRegion::Static => {}
                }
            }
            Some(())
        }
        fn go(&mut self, ty: Ty, row: &[FnRegion], at: &mut usize, variance: V) -> Option<()> {
            if let Some(&FnRegion::Projection(i)) = row.get(*at) {
                let alias = self.r.projections.get(i as usize)?.alias;
                let original = Ty::alias(alias);
                if ty == original
                    || ty == flux_solve::traits::normalize(self.items, self.sigs, &[], original)
                {
                    self.projection(i, 0)?;
                    *at += 1;
                    return Some(());
                }
            }
            if let Some(&FnRegion::Type(i)) = row.get(*at)
                && self.r.types.get(i as usize) == Some(&ty)
            {
                self.types[i as usize] = self.types[i as usize].join(variance);
                *at += 1;
                return Some(());
            }
            match ty.kind() {
                TyKind::Ref(m, inner) => {
                    self.slot(row, at, variance)?;
                    self.go(
                        inner,
                        row,
                        at,
                        variance.xform(if m == crate::Mutability::Mut {
                            V::Invariant
                        } else {
                            V::Covariant
                        }),
                    )?;
                }
                TyKind::RawPtr(m, inner) => self.go(
                    inner,
                    row,
                    at,
                    variance.xform(if m == crate::Mutability::Mut {
                        V::Invariant
                    } else {
                        V::Covariant
                    }),
                )?,
                TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                    self.go(inner, row, at, variance)?
                }
                TyKind::Tuple(types) => {
                    for &ty in types {
                        self.go(ty, row, at, variance)?;
                    }
                }
                TyKind::Dynamic(object) => {
                    self.slot(row, at, variance)?;
                    for ty in crate::regions::object_region_types(self.items, object) {
                        self.go(ty, row, at, variance.xform(V::Invariant))?;
                    }
                }
                TyKind::Adt(adt, args) => {
                    let lifetimes = self.items.structs[adt.strukt].generics.lifetimes.len();
                    let table = self.sigs.regions.structs.get(adt.strukt);
                    let of = |i| {
                        table
                            .and_then(|t| t.variances.get(i))
                            .copied()
                            .unwrap_or(V::Invariant)
                    };
                    for i in 0..lifetimes {
                        self.slot(row, at, variance.xform(of(i)))?;
                    }
                    for (i, &ty) in args.iter().enumerate() {
                        self.go(ty, row, at, variance.xform(of(lifetimes + i)))?;
                    }
                }
                _ if crate::regions::slots(self.items, ty) != 0 => return None,
                _ => {}
            }
            Some(())
        }
    }
    let r = sig.regions?;
    if sig.inputs_output.len() != r.inputs_output.len() {
        return None;
    }
    let mut cx = Walk {
        items,
        sigs,
        r,
        free: vec![V::Bivariant; r.free as usize],
        types: vec![V::Bivariant; r.types.len()],
    };
    for (i, (&ty, row)) in sig.inputs_output.iter().zip(r.inputs_output).enumerate() {
        let mut at = 0;
        cx.go(
            ty,
            row,
            &mut at,
            if i < sig.params().len() {
                V::Contravariant
            } else {
                V::Covariant
            },
        )?;
        if at != row.len() {
            return None;
        }
    }
    Some((cx.free, cx.types))
}
