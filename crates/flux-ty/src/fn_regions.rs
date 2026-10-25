//! Canonical region recipes for bare function-pointer types.
use flux_hir::{ItemTree, LifetimeRef, ModuleId, TypeRef};
use flux_intern::Symbol;
use flux_syntax::TextRange;
use crate::{FnPtrRegions, FnRegion, Ty, TyKind};

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
    errors: &'a mut Vec<(TextRange, String)>,
}

pub(crate) fn describe(
    items: &ItemTree, module: ModuleId, names: &[Symbol],
    params: &[TypeRef], ret: &TypeRef, types: &[Ty], output: Ty,
    errors: &mut Vec<(TextRange, String)>,
) -> Description {
    for (i, name) in names.iter().enumerate() {
        if names[..i].contains(name) {
            errors.push((TextRange::default(), "lifetime name is already in use".into()));
        }
    }
    let mut cx = Builder { items, module, names: names.iter().map(|&n| (n, None)).collect(),
        bound: 0, free: Vec::new(), captures: Vec::new(), inputs: Vec::new(), output: false, enclosing: None, errors };
    let mut rows = Vec::new();
    for (written, &ty) in params.iter().zip(types) {
        let row = cx.walk(written, ty);
        for &region in &row {
            if !matches!(region, FnRegion::Type(_)) && !cx.inputs.contains(&region) { cx.inputs.push(region); }
        }
        rows.push(row);
    }
    cx.output = true;
    rows.push(cx.walk(ret, output));
    if rows.last().is_some_and(|row| row.iter().any(|r| matches!(r, FnRegion::Bound(_)) && !cx.inputs.contains(r))) {
        cx.errors.push((TextRange::default(), "return lifetime is not constrained by the function inputs".into()));
    }
    let types: Vec<_> = cx.captures.iter().map(|(_, ty)| *ty).collect();
    let regions = FnPtrRegions::intern(cx.bound, cx.free.len() as u32, &rows, &types);
    Description { regions, free: cx.free, captures: cx.captures }
}

impl Builder<'_> {
    fn lifetime(&mut self, lifetime: Option<LifetimeRef>, at: TextRange) -> FnRegion {
        if let Some(lifetime) = lifetime.filter(|l| !l.is_anonymous()) {
            if lifetime.is_static() { return FnRegion::Static; }
            if let Some(index) = self.names.iter().position(|(name, _)| *name == lifetime.name) {
                let bound = match self.names[index].1 {
                    Some(bound) => bound,
                    None => { let next = self.bound; self.bound += 1; self.names[index].1 = Some(next); next }
                };
                return FnRegion::Bound(bound);
            }
            let index = match self.free.iter().position(|l| l.name == lifetime.name) {
                Some(i) => i,
                None => { self.free.push(lifetime); self.free.len() - 1 }
            };
            return FnRegion::Free(index as u32);
        }
        if self.output {
            if let [region] = &*self.inputs { return *region; }
            self.errors.push((at, "missing lifetime specifier".into()));
            // The error prevents admission; this placeholder is never a proof.
            return FnRegion::Static;
        }
        let next = self.bound; self.bound += 1;
        FnRegion::Bound(next)
    }

    fn capture(&mut self, written: &TypeRef, ty: Ty) -> Vec<FnRegion> {
        if !ty.has_params() && !ty.has_infer() && !ty.has_aliases()
            && crate::regions::slots(self.items, ty) == 0 { return Vec::new(); }
        // Repeated occurrences of one type parameter share its stored regions.
        // Distinct parameters must remain separate even after substitution.
        if matches!(ty.kind(), TyKind::Param(_))
            && let Some(index) = self.captures.iter().position(|(_, captured)| *captured == ty) {
            return vec![FnRegion::Type(index as u32)];
        }
        let index = self.captures.len();
        self.captures.push((written.clone(), ty));
        vec![FnRegion::Type(index as u32)]
    }

    fn walk(&mut self, written: &TypeRef, ty: Ty) -> Vec<FnRegion> {
        if let TypeRef::Path(path) = written
            && matches!(self.items.resolve(self.module, path, flux_hir::def_map::Ns::Types),
                Ok(flux_hir::def_map::Resolved::Def(flux_hir::def_map::Def::TypeAlias(_)))) {
            if crate::regions::slots(self.items, ty) != 0 {
                self.errors.push((path.range(path.segments.len().saturating_sub(1), TextRange::default()),
                    "region-bearing aliases inside function pointers are not supported yet".into()));
            }
            return self.capture(written, ty);
        }
        match (written, ty.kind()) {
            (TypeRef::Ref { lifetime, amp, pointee, .. }, TyKind::Ref(_, inner)) => {
                let region = self.lifetime(*lifetime, *amp);
                let previous = self.enclosing.replace(region);
                let mut row = vec![region];
                row.extend(self.walk(pointee, inner));
                self.enclosing = previous;
                row
            }
            (TypeRef::Dyn(d), TyKind::Dynamic(dynamic)) => {
                if dynamic.args.iter().chain(dynamic.assoc_tys).any(|&t|
                    t.has_params() || crate::regions::slots(self.items, t) != 0) {
                    self.errors.push((TextRange::default(), "trait object argument regions inside function pointers are not supported yet".into()));
                }
                for bound in &d.bounds {
                    let (trait_, _) = self.items.resolve_trait(self.module, bound);
                    if trait_.is_some_and(|id| !self.items.traits[id].generics.lifetimes.is_empty())
                        || bound.path.args.iter().flatten().any(|a| !a.lifetimes.is_empty()) {
                        self.errors.push((bound.range, "lifetime arguments on trait objects are not supported yet".into()));
                    }
                }
                vec![match d.lifetime {
                    Some(l) => self.lifetime(Some(l), l.range),
                    None => self.enclosing.unwrap_or(FnRegion::Static),
                }]
            }
            (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(_, inner)) => self.walk(pointee, inner),
            (TypeRef::Array { elem, .. }, TyKind::Array(inner, _))
                | (TypeRef::Slice(elem), TyKind::Slice(inner)) => self.walk(elem, inner),
            (TypeRef::Tuple(written), TyKind::Tuple(types)) => {
                let mut row = Vec::new();
                for (written, &ty) in written.iter().zip(types) { row.extend(self.walk(written, ty)); }
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
                if sig.regions.is_some_and(|r| r.free != 0 || !r.types.is_empty()) {
                    self.errors.push((TextRange::default(), "captured regions in nested function pointers are not supported yet".into()));
                }
                self.capture(written, ty)
            }
            _ => self.capture(written, ty),
        }
    }
}

/// Instantiate only a pointer's relative recipe. Bound variables are supplied
/// freshly by the call/proof; own variables are free slots then captured types.
pub fn instantiate<R: Copy>(items: &ItemTree, sig: crate::FnSigTy, own: &[R], bound: &[R], static_region: R) -> Option<Vec<Vec<R>>> {
    let r = sig.regions?;
    if bound.len() != r.bound as usize || r.inputs_output.len() != sig.inputs_output.len() { return None; }
    let free = own.get(..r.free as usize)?;
    let mut at = r.free as usize;
    let mut captures = Vec::new();
    for &ty in r.types {
        let n = crate::regions::slots(items, ty);
        captures.push(own.get(at..at+n)?); at += n;
    }
    if at != own.len() { return None; }
    let mut rows = Vec::new();
    for (&ty, recipe) in sig.inputs_output.iter().zip(r.inputs_output) {
        let mut row = Vec::new();
        for term in *recipe {
            match *term {
                FnRegion::Static => row.push(static_region),
                FnRegion::Bound(i) => row.push(*bound.get(i as usize)?),
                FnRegion::Free(i) => row.push(*free.get(i as usize)?),
                FnRegion::Type(i) => row.extend_from_slice(captures.get(i as usize)?),
            }
        }
        if row.len() != crate::regions::slots(items, ty) { return None; }
        rows.push(row);
    }
    Some(rows)
}

/// Variance of the value's free regions and captured type groups. Bound
/// lifetimes are not value slots and must never consume an outer declaration's
/// region table while computing the variance of a field containing a pointer.
pub fn variances(items: &ItemTree, sigs: &crate::Signatures, sig: crate::FnSigTy) -> Option<(Vec<crate::regions::Variance>, Vec<crate::regions::Variance>)> {
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
            let term = row.get(*at)?; *at += 1;
            match *term {
                FnRegion::Free(i) => { let slot = self.free.get_mut(i as usize)?; *slot = slot.join(variance); }
                FnRegion::Bound(_) | FnRegion::Static => {}
                FnRegion::Type(_) => return None,
            }
            Some(())
        }
        fn go(&mut self, ty: Ty, row: &[FnRegion], at: &mut usize, variance: V) -> Option<()> {
            if let Some(&FnRegion::Type(i)) = row.get(*at)
                && self.r.types.get(i as usize) == Some(&ty) {
                self.types[i as usize] = self.types[i as usize].join(variance);
                *at += 1; return Some(());
            }
            match ty.kind() {
                TyKind::Ref(m, inner) => {
                    self.slot(row, at, variance)?;
                    self.go(inner, row, at, variance.xform(if m == crate::Mutability::Mut { V::Invariant } else { V::Covariant }))?;
                }
                TyKind::RawPtr(m, inner) => self.go(inner, row, at, variance.xform(if m == crate::Mutability::Mut { V::Invariant } else { V::Covariant }))?,
                TyKind::Array(inner, _) | TyKind::Slice(inner) => self.go(inner, row, at, variance)?,
                TyKind::Tuple(types) => for &ty in types { self.go(ty, row, at, variance)?; },
                TyKind::Dynamic(_) => self.slot(row, at, variance)?,
                TyKind::Adt(adt, args) => {
                    let lifetimes = self.items.structs[adt.strukt].generics.lifetimes.len();
                    let table = self.sigs.regions.structs.get(adt.strukt);
                    let of = |i| table.and_then(|t| t.variances.get(i)).copied().unwrap_or(V::Invariant);
                    for i in 0..lifetimes { self.slot(row, at, variance.xform(of(i)))?; }
                    for (i, &ty) in args.iter().enumerate() { self.go(ty, row, at, variance.xform(of(lifetimes+i)))?; }
                }
                _ if crate::regions::slots(self.items, ty) != 0 => return None,
                _ => {}
            }
            Some(())
        }
    }
    let r = sig.regions?;
    if sig.inputs_output.len() != r.inputs_output.len() { return None; }
    let mut cx = Walk { items, sigs, r, free: vec![V::Bivariant; r.free as usize], types: vec![V::Bivariant; r.types.len()] };
    for (i, (&ty, row)) in sig.inputs_output.iter().zip(r.inputs_output).enumerate() {
        let mut at = 0;
        cx.go(ty, row, &mut at, if i < sig.params().len() { V::Contravariant } else { V::Covariant })?;
        if at != row.len() { return None; }
    }
    Some((cx.free, cx.types))
}

/// Equality of binder positions for trait/header matching. Early regions are
/// deliberately all external here; their equality is proven by the caller's
/// region tables. A bound region can never match an external region.
pub fn same_binders(items: &ItemTree, a: Ty, b: Ty) -> bool {
    if a == b || a.has_infer() || b.has_infer() { return true; }
    if let (TyKind::FnPtr(a), TyKind::FnPtr(b)) = (a.kind(), b.kind()) {
        let shape = |sig: crate::FnSigTy| {
            if let Some(r) = sig.regions {
                let own = vec![None; crate::regions::slots(items, Ty::new(TyKind::FnPtr(sig)))];
                let bound: Vec<_> = (0..r.bound).map(Some).collect();
                instantiate(items, sig, &own, &bound, None)
            } else if sig.inputs_output.iter().all(|&t| crate::regions::slots(items, t) == 0) {
                Some(vec![Vec::new(); sig.inputs_output.len()])
            } else { None }
        };
        let (Some(x), Some(y)) = (shape(a), shape(b)) else { return false; };
        if x != y { return false; }
    }
    // Ordinary type matching/normalization owns all non-callable shapes.
    // This guard only adds binder identity at corresponding callable nodes.
    a.children().zip(b.children()).all(|(a, b)| same_binders(items, a, b))
}

/// Type equality after erasing only function-pointer region recipes. This is
/// used before MIR checks callable subtyping, and for MIR storage validation.
pub fn same_shape(a: Ty, b: Ty) -> bool {
    if a == b { return true; }
    let all = |a: &[Ty], b: &[Ty]| a.len() == b.len()
        && a.iter().zip(b).all(|(&a, &b)| same_shape(a, b));
    match (a.kind(), b.kind()) {
        (TyKind::FnPtr(a), TyKind::FnPtr(b)) => a.is_c == b.is_c
            && a.is_unsafe == b.is_unsafe && all(a.inputs_output, b.inputs_output),
        (TyKind::Ref(m, a), TyKind::Ref(n, b))
            | (TyKind::RawPtr(m, a), TyKind::RawPtr(n, b)) => m == n && same_shape(a, b),
        (TyKind::Array(a, n), TyKind::Array(b, m)) => n == m && same_shape(a, b),
        (TyKind::Slice(a), TyKind::Slice(b)) => same_shape(a, b),
        (TyKind::Tuple(a), TyKind::Tuple(b)) => all(a, b),
        (TyKind::Adt(a, x), TyKind::Adt(b, y)) => a == b && all(x, y),
        (TyKind::Closure(a, x), TyKind::Closure(b, y)) => a == b && all(x, y),
        (TyKind::FnDef(a), TyKind::FnDef(b)) => a.function == b.function && all(a.args, b.args),
        (TyKind::Alias(a), TyKind::Alias(b)) => a.trait_ == b.trait_ && a.name == b.name && all(a.args, b.args),
        _ => false,
    }
}
