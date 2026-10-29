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
            if !matches!(region, FnRegion::Type(_)) && !cx.inputs.contains(&region) {
                cx.inputs.push(region);
            }
        }
        rows.push(row);
    }
    cx.output = true;
    rows.push(cx.walk(ret, signature.ret()));
    if rows.last().is_some_and(|row| {
        row.iter()
            .any(|r| matches!(r, FnRegion::Bound(_)) && !cx.inputs.contains(r))
    }) {
        cx.errors.push((
            TextRange::default(),
            "return lifetime is not constrained by the function inputs".into(),
        ));
    }
    let types: Vec<_> = cx.captures.iter().map(|(_, ty)| *ty).collect();
    let regions = FnPtrRegions::intern(cx.bound, cx.free.len() as u32, &rows, &types);
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
            && matches!(
                self.items
                    .resolve(self.module, path, flux_hir::def_map::Ns::Types),
                Ok(flux_hir::def_map::Resolved::Def(
                    flux_hir::def_map::Def::TypeAlias(_)
                ))
            )
        {
            if crate::regions::slots(self.items, ty) != 0 {
                self.errors.push((
                    path.range(path.segments.len().saturating_sub(1), TextRange::default()),
                    "region-bearing aliases inside function pointers are not supported yet".into(),
                ));
            }
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
                if dynamic
                    .args
                    .iter()
                    .chain(dynamic.assoc_tys)
                    .any(|&t| t.has_params() || crate::regions::slots(self.items, t) != 0)
                {
                    self.errors.push((TextRange::default(), "trait object argument regions inside function pointers are not supported yet".into()));
                }
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
                vec![match d.lifetime {
                    Some(l) => self.lifetime(Some(l), l.range),
                    None if self.elision_only => return Vec::new(),
                    None if crate::regions::object_has_static_self(self.items, dynamic.trait_) => {
                        FnRegion::Static
                    }
                    None => enclosing.unwrap_or(FnRegion::Static),
                }]
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
                if sig
                    .regions
                    .is_some_and(|r| r.free != 0 || !r.types.is_empty())
                {
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
                FnRegion::Type(_) => return None,
            }
            Some(())
        }
        fn go(&mut self, ty: Ty, row: &[FnRegion], at: &mut usize, variance: V) -> Option<()> {
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
                TyKind::Dynamic(_) => self.slot(row, at, variance)?,
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
