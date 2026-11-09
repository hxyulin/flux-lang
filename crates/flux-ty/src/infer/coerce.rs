//! Value coercions and their noncommitting inference probes.
use super::{
    Ctx, Expr, ExprId, FxHashSet, Mutability, OverloadedDeref, Predicate, TextRange, Ty, TyKind,
    VarValue, plural, solve,
};

impl Ctx<'_> {
    /// Checks that a value of type `actual` (from `expr`) can be used where
    /// `expected` is: `!` coerces to anything, other types must unify.
    pub(super) fn coerce(&mut self, actual: Ty, expected: Ty, expr: ExprId) {
        if self.shallow(actual) == Ty::NEVER {
            self.never_coercions.insert(expr, expected);
            if let TyKind::Infer(var) = self.shallow(expected).kind() {
                let (int, _) = self.unknown(var);
                self.set_var(
                    var,
                    VarValue::Unknown {
                        int,
                        float: self.float_var(var),
                        diverging: true,
                    },
                );
            }
            return;
        }
        let blamed = self.blame(expr);
        // A block ending with a reference: its tail is reborrowed or
        // unsized where it is, inside the block (`{ self.value }` of a
        // `&mut T` field where a `&T` is wanted), and the block has the
        // type it is coerced to.
        if blamed != expr
            && let TyKind::Ref(..) = self.shallow(actual).kind()
            && let TyKind::Ref(..) | TyKind::RawPtr(..) = self.shallow(expected).kind()
            && let Some(&tail_ty) = self.expr_types.get(blamed)
        {
            self.coerce(tail_ty, expected, blamed);
            let coerced = match (
                self.reborrows.get(blamed).is_some(),
                self.unsizes.get(blamed).is_some(),
            ) {
                (false, false) => return,
                _ => expected,
            };
            let mut at = expr;
            while at != blamed {
                self.expr_types.insert(at, coerced);
                match self.body.exprs[at] {
                    Expr::Block {
                        tail: Some(tail), ..
                    } => at = tail,
                    _ => break,
                }
            }
            return;
        }
        // A reference to an array where one to a slice is expected is
        // unsized, after a reborrow if it is a `&mut` (which may become a
        // `&`): rustc's unsizing coercion.
        if let Some((from, array, to, slice, reborrow)) =
            match (self.shallow(actual).kind(), self.shallow(expected).kind()) {
                (TyKind::Ref(from, a), TyKind::Ref(to, b)) => Some((from, a, to, b, true)),
                (TyKind::Ref(from, a), TyKind::RawPtr(to, b)) => Some((from, a, to, b, true)),
                (TyKind::RawPtr(from, a), TyKind::RawPtr(to, b)) => Some((from, a, to, b, false)),
                _ => None,
            }
            && (from == to || from == Mutability::Mut)
            && let Some(equalities) = self.array_unsize(array, slice)
        {
            if from == Mutability::Mut && reborrow {
                self.reborrows.insert(expr, to);
            }
            self.unsizes.insert(expr, expected);
            if !equalities.into_iter().all(|(a, b)| self.unify(a, b)) {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A reference or raw pointer to a value where one to a trait
        // object is expected is unsized, after a reborrow if it is a
        // `&mut`, if the value's type implements the trait; one to
        // another trait object is upcast to its supertrait's: rustc's
        // unsizing coercions, tried before deref coercion as in rustc.
        if let Some((from, to, pointee, target, raw)) =
            match (self.shallow(actual).kind(), self.shallow(expected).kind()) {
                (TyKind::Ref(from, pointee), TyKind::Ref(to, target)) => {
                    Some((from, to, pointee, target, false))
                }
                (TyKind::RawPtr(from, pointee), TyKind::RawPtr(to, target))
                | (TyKind::Ref(from, pointee), TyKind::RawPtr(to, target)) => {
                    Some((from, to, pointee, target, true))
                }
                _ => None,
            }
            && let Some((pointee, target)) = flux_solve::transparent_object_unsize(
                self.items,
                &self.sigs.structs,
                self.shallow(pointee),
                self.shallow(target),
            )
            && let TyKind::Dynamic(d) = target.kind()
            && (from == to || from == Mutability::Mut)
        {
            let target = self.shallow(target);
            let pointee = self.shallow(pointee);
            if !self.can_unify(pointee, target) {
                if self.unsize_to_dyn(pointee, target, d, blamed) {
                    if from == Mutability::Mut && !raw {
                        self.reborrows.insert(expr, to);
                    }
                    self.unsizes.insert(expr, expected);
                } else {
                    let range = self.range(blamed);
                    self.mismatch(expected, actual, range);
                }
                return;
            }
        }
        // NonNull carries a covariant raw pointer; only the trusted wrapper
        // receives this builtin array-to-slice coercion.
        if let (Some(from), Some(to)) = (
            crate::nonnull_pointee(self.items, self.shallow(actual)),
            crate::nonnull_pointee(self.items, self.shallow(expected)),
        ) && let Some(equalities) = self.array_unsize(from, to)
        {
            if equalities.into_iter().all(|(a, b)| self.unify(a, b)) {
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A `Box<[T; N]>` or `Box<T>` where a `Box<[T]>` or `Box<dyn
        // Tr>` is wanted: rustc's `CoerceUnsized` of boxes.
        if let (Some(p), Some(q)) = (
            flux_solve::box_pointee(self.shallow(actual)),
            flux_solve::box_pointee(self.shallow(expected)),
        ) {
            let (p, q) = (self.shallow(p), self.shallow(q));
            if let Some(equalities) = self.array_unsize(p, q) {
                if equalities.into_iter().all(|(a, b)| self.unify(a, b)) {
                    self.unsizes.insert(expr, expected);
                } else {
                    let range = self.range(blamed);
                    self.mismatch(expected, actual, range);
                }
                return;
            }
            if !q.is_dyn()
                && let Some((source, target)) =
                    flux_solve::transparent_object_unsize(self.items, &self.sigs.structs, p, q)
                && let TyKind::Dynamic(object) = target.kind()
            {
                if self.unsize_to_dyn(source, target, object, blamed) {
                    self.unsizes.insert(expr, expected);
                } else {
                    let range = self.range(blamed);
                    self.mismatch(expected, actual, range);
                }
                return;
            }
            match (p.kind(), q.kind()) {
                (TyKind::Array(elem, _), TyKind::Slice(wanted)) => {
                    if self.unify(elem, wanted) {
                        self.unsizes.insert(expr, expected);
                    } else {
                        let range = self.range(blamed);
                        self.mismatch(expected, actual, range);
                    }
                    return;
                }
                (_, TyKind::Dynamic(d)) if !self.can_unify(p, q) => {
                    if self.unsize_to_dyn(p, q, d, blamed) {
                        self.unsizes.insert(expr, expected);
                    } else {
                        let range = self.range(blamed);
                        self.mismatch(expected, actual, range);
                    }
                    return;
                }
                _ => {}
            }
        }
        // Arc and Rc object coercions transfer one owner without touching counts.
        if let Some((source, object)) =
            crate::shared_pointees(self.items, self.shallow(actual), self.shallow(expected))
            && let TyKind::Dynamic(d) = object.kind()
        {
            if self.unsize_to_dyn(source, object, d, blamed) {
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // Moving an array Arc or Rc into a slice one preserves its shared header
        // and data address; only the data pointer gains length metadata.
        if let Some((array, slice)) =
            crate::shared_pointees(self.items, self.shallow(actual), self.shallow(expected))
            && let (TyKind::Array(elem, _), TyKind::Slice(wanted)) =
                (self.shallow(array).kind(), self.shallow(slice).kind())
        {
            if self.unify(elem, wanted) {
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A `&&T`, `&mut &T` or `&&mut T` where a `&T` is expected is
        // dereferenced until it is one, then reborrowed: rustc's deref
        // coercion, through references.
        if let (TyKind::Ref(from, pointee), TyKind::Ref(to, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
            && (from == to || from == Mutability::Mut)
            && !self.can_unify(pointee, expected_pointee)
        {
            let range = self.range(blamed);
            if let Some((ty, derefs, steps)) =
                self.deref_coercion(from, pointee, to, expected_pointee, range)
            {
                self.unify(ty, expected_pointee);
                self.reborrows.insert(expr, to);
                self.deref_coercions.insert(expr, derefs);
                if !steps.is_empty() {
                    for step in &steps {
                        self.register_deref(step.ty, step.mutability, range);
                    }
                    self.coercion_derefs.insert(expr, steps);
                }
                return;
            }
        }
        // A `&mut T` where a reference is expected is reborrowed, and may
        // become a `&T`.
        if let (TyKind::Ref(Mutability::Mut, pointee), TyKind::Ref(m, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            self.reborrows.insert(expr, m);
            if m == Mutability::Mut {
                self.mutable_place(expr, false, self.range(blamed));
            }
            if !self.unify(pointee, expected_pointee) {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A closure where a function pointer is wanted becomes one, if
        // it captures nothing (checked once that is known).
        if let (TyKind::Closure(_, args), TyKind::FnPtr(ptr)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            let parts = crate::ClosureArgs::split(args);
            let params_ok = self.unify(parts.params(), Ty::tuple(ptr.params()));
            if ptr.abi == flux_hir::Abi::Rust && params_ok && self.unify(parts.ret(), ptr.ret()) {
                self.reified.insert(expr, expected);
                self.reified_closures.push((expr, actual, expected));
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A function item where a function pointer is wanted becomes
        // one, unless it is unsafe and the pointer is not.
        if let (TyKind::FnDef(def), TyKind::FnPtr(ptr)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            if self.items.functions[def.function].intrinsic.is_some() {
                self.error(
                    self.range(blamed),
                    "cannot coerce intrinsics to function pointers",
                );
                return;
            }
            if !ptr.is_unsafe && !self.can_reify_function_features(def.function) {
                self.error(self.range(blamed),"cannot coerce a target-feature function to a safe function pointer outside its feature context");
                return;
            }
            // A method of a trait object is called through its vtable,
            // which a function pointer would need a shim for.
            if matches!(
                self.items.functions[def.function].owner,
                Some(flux_hir::Owner::Trait(_))
            ) && def.args.first().is_some_and(|&a| self.shallow(a).is_dyn())
            {
                self.error(
                    self.range(blamed),
                    "function pointers to trait objects' methods are not supported yet",
                );
                return;
            }
            let abi = self.items.functions[def.function].abi;
            // Pointer input references are independently higher-ranked.
            // Fixed or constrained lifetimes in the item's signature cannot
            // be discarded when reifying it, including after `Clone`.
            // Trait predicates can bind an own lifetime even when their
            // normalized types erase it; explicit outlives bounds alone
            // do not classify the binder.
            let Some(regions) = self.sigs.regions.fns.get(def.function) else {
                self.error(
                    self.range(blamed),
                    "function item pointer region metadata is not supported yet",
                );
                return;
            };
            if regions.callback_binder == crate::regions::CallbackBinder::Unclassified {
                self.error(
                    self.range(blamed),
                    "function item pointer region metadata is not supported yet",
                );
                return;
            }
            let mut input_lifetimes = FxHashSet::default();
            if ptr.regions.is_none()
                && (regions.callback_binder == crate::regions::CallbackBinder::SignatureBound
                    || regions.params.iter().flatten().any(|r| match r {
                        crate::regions::Region::Static => true,
                        crate::regions::Region::Param(i) => {
                            *i < regions.owner || !input_lifetimes.insert(*i)
                        }
                        _ => false,
                    }))
            {
                self.error(
                    self.range(blamed),
                    "function pointers to functions with constrained input lifetimes are not supported yet",
                );
                return;
            }
            let reified = Ty::new(TyKind::FnPtr(crate::FnSigTy {
                is_unsafe: ptr.is_unsafe,
                ..def.sig
            }));
            if ptr.abi == abi
                && (ptr.is_unsafe || !def.sig.is_unsafe)
                && self.unify(reified, expected)
            {
                self.reified.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // Pointer binder coercions have no runtime cost, but MIR must retain
        // a cast between their distinct lifetime recipes for the region proof.
        if let (TyKind::FnPtr(from), TyKind::FnPtr(to)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            let offered = Ty::new(TyKind::FnPtr(crate::FnSigTy {
                is_unsafe: to.is_unsafe,
                ..from
            }));
            if from.abi == to.abi
                && (to.is_unsafe || !from.is_unsafe)
                && self.unify(offered, expected)
            {
                self.reified.insert(expr, expected);
            } else {
                self.mismatch(expected, actual, self.range(blamed));
            }
            return;
        }
        // A `*mut T` where a `*const T` is wanted becomes one, cast.
        if let (TyKind::RawPtr(Mutability::Mut, pointee), TyKind::RawPtr(Mutability::Not, wanted)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            if self.unify(pointee, wanted) {
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A reference where a raw pointer is wanted becomes one, after
        // a reborrow if it is a `&mut`: `&T` to `*const T`, `&mut T` to
        // `*mut T` or `*const T`.
        if let (TyKind::Ref(from, pointee), TyKind::RawPtr(to, expected_pointee)) =
            (self.shallow(actual).kind(), self.shallow(expected).kind())
        {
            // A same-object pointer retains the object's represented lifetime;
            // MIR relates that row independently of the reference's borrow.
            if (from == to || from == Mutability::Mut) && self.unify(pointee, expected_pointee) {
                if from == Mutability::Mut {
                    self.reborrows.insert(expr, to);
                }
                self.unsizes.insert(expr, expected);
            } else {
                let range = self.range(blamed);
                self.mismatch(expected, actual, range);
            }
            return;
        }
        // A tuple expression is checked element by element, as rustc
        // does, so a mismatch is reported at the element, and each
        // element may be coerced: the tuple then has the type it is
        // coerced to (one whose context wanted a tuple was already).
        if let Expr::Tuple(fields) = &self.body.exprs[blamed]
            && !self.coerced_tuples.contains(&blamed)
            && let TyKind::Tuple(elements) = self.shallow(expected).kind()
        {
            if fields.len() == elements.len() {
                for (&field, &element) in self.body.lists[*fields].iter().zip(elements) {
                    let ty = self.expr_types.get(field).copied().unwrap_or(Ty::ERROR);
                    self.coerce(ty, element, field);
                }
                self.retype(expr, blamed, Ty::tuple(elements));
            } else {
                let (want, got) = (elements.len(), fields.len());
                self.error(
                    self.range(blamed),
                    format!(
                        "mismatched types: expected a tuple with {want} element{}, found one with {got} element{}",
                        plural(want),
                        plural(got)
                    ),
                );
            }
            return;
        }
        if !self.unify(actual, expected) {
            let range = self.range(blamed);
            if let Expr::Try { .. } = self.body.exprs[blamed] {
                let (expected, actual) = (self.describe(expected), self.describe(actual));
                self.error(
                    range,
                    format!(
                        "`?` operator has incompatible types: expected `{expected}`, found `{actual}`"
                    ),
                );
                return;
            }
            self.mismatch(expected, actual, range);
        }
    }

    /// Whether a pointer to a `source` may become one to `target`, trait
    /// object `d`: `source` must implement its trait with the associated
    /// types it gives, and be sized (obligations, reported at `expr`
    /// if they fail); or be a trait object whose trait has `d`'s as a
    /// supertrait (upcasting).
    pub(super) fn unsize_to_dyn(
        &mut self,
        source: Ty,
        target: Ty,
        d: crate::DynTy,
        expr: ExprId,
    ) -> bool {
        let range = self.range(expr);
        let mut callable_source = source;
        while let TyKind::Ref(_, inner) = callable_source.kind() {
            callable_source = inner;
        }
        let caller_callable = matches!(callable_source.kind(), TyKind::Param(_) | TyKind::Alias(_));
        if crate::fn_trait_kind(self.items, d.trait_).is_some()
            && d.args
                .iter()
                .any(|&ty| crate::regions::slots(self.items, ty) != 0)
            && !matches!(
                source.kind(),
                TyKind::Closure(..) | TyKind::Dynamic(_) | TyKind::FnDef(_) | TyKind::FnPtr(_)
            )
            && !caller_callable
        {
            self.error(range, "higher-ranked callable object coercions from non-closure types are not supported yet");
            return true;
        }
        if caller_callable
            && d.assoc_tys
                .iter()
                .any(|&ty| crate::regions::slots(self.items, ty) != 0)
        {
            self.error(
                range,
                "callable object outputs with references are not supported yet",
            );
            return true;
        }
        if let TyKind::Dynamic(s) = source.kind() {
            if !crate::objects::retains_auto_traits(self.sigs, source, target) {
                return false;
            }
            let principal = flux_solve::traits::dyn_principal(target, d);
            let found = flux_solve::traits::dyn_bounds(self.sigs, source, s)
                .into_iter()
                .find(|p| p.trait_ref.trait_ == d.trait_);
            let Some(found) = found else {
                return false;
            };
            let mut ok = true;
            for (&a, &b) in found.trait_ref.args.iter().zip(principal.trait_ref.args) {
                ok &= self.unify(a, b);
            }
            // Its associated types are the source's.
            for (&name, &ty) in d.assoc_names.iter().zip(d.assoc_tys) {
                let given = flux_solve::traits::dyn_projections(self.items, self.sigs, source, s)
                    .into_iter()
                    .find(|p| p.alias.name == name)
                    .map(|p| p.ty);
                ok &= given.is_some_and(|g| self.unify(g, ty));
            }
            return ok;
        }
        // A closure whose signature did not fit was reported.
        if let TyKind::Closure(id, _) = source.kind()
            && self.mismatched_closures.contains(&id.expr)
        {
            return true;
        }
        let principal = flux_solve::traits::dyn_principal(source, d);
        self.register(principal, range, solve::ObligationKind::Bound);
        for &trait_ref in d.auto_traits {
            self.register(
                Predicate {
                    self_ty: source,
                    trait_ref,
                },
                range,
                solve::ObligationKind::Bound,
            );
        }
        for p in flux_solve::traits::dyn_projections(self.items, self.sigs, target, d) {
            if p.alias.args[0] == target && d.assoc_names.contains(&p.alias.name) {
                let alias = flux_solve::traits::alias_of(
                    self.items,
                    crate::TraitRef {
                        trait_: p.alias.trait_,
                        name: p.alias.trait_name,
                        args: Ty::list(&p.alias.args[1..]),
                    },
                    source,
                    p.alias.name,
                );
                if self.is_unsized(source) {
                    break;
                }
                self.register_projection(crate::Projection { alias, ty: p.ty }, range);
            }
        }
        if let Some(sized) = self.sigs.builtins.sized {
            let p = Predicate {
                self_ty: source,
                trait_ref: crate::TraitRef {
                    trait_: sized,
                    name: self.items.traits[sized].name,
                    args: &[],
                },
            };
            self.register(p, range, solve::ObligationKind::Bound);
        }
        true
    }

    /// rustc's deref coercion of a `&pointee` (or `&mut`, `from`) to a
    /// `&expected` (`to`): the type `pointee` dereferences to that may
    /// be `expected`, through references and `Deref` (`DerefMut` for a
    /// `&mut`), with how many dereferences that is, the reference
    /// included, and the overloaded ones among them. A `&mut` comes
    /// only through `&mut` references.
    pub(super) fn deref_coercion(
        &mut self,
        from: Mutability,
        pointee: Ty,
        to: Mutability,
        expected: Ty,
        range: TextRange,
    ) -> Option<(Ty, u32, Vec<OverloadedDeref>)> {
        let (mut ty, mut derefs, mut unique) = (pointee, 1, from == Mutability::Mut);
        let mut steps = Vec::new();
        while derefs < 128 {
            let current = self.shallow(ty);
            let (next, overloaded) = self.deref_step(current, range)?;
            if overloaded {
                // `DerefMut` for a `&mut`, where the type has it; else
                // `Deref`, which the borrow checker finds borrowed
                // mutably (E0596), as in rustc.
                let deref_mut = self.deref_trait(true);
                let mutability = if to == Mutability::Mut
                    && deref_mut
                        .is_some_and(|t| self.probe(|cx| cx.may_hold(current, t, Some(&[]), 0)))
                {
                    Mutability::Mut
                } else {
                    Mutability::Not
                };
                steps.push(OverloadedDeref {
                    step: derefs,
                    mutability,
                    ty: current,
                    target: next,
                });
            } else if let TyKind::Ref(m, _) = current.kind() {
                unique &= m == Mutability::Mut;
            }
            (ty, derefs) = (next, derefs + 1);
            if (to == Mutability::Not || unique) && self.can_unify(ty, expected) {
                return Some((ty, derefs, steps));
            }
        }
        None
    }

    /// Whether `coerce` would accept a value of type `actual` from
    /// `expr` where `expected` is wanted, without doing anything.
    pub(super) fn can_coerce(&mut self, actual: Ty, expected: Ty, expr: ExprId) -> bool {
        let (actual, expected) = (self.shallow(actual), self.shallow(expected));
        if actual == Ty::NEVER {
            return true;
        }
        if let Some((source, target)) = crate::shared_pointees(self.items, actual, expected)
            && let TyKind::Dynamic(object) = target.kind()
        {
            return match source.kind() {
                TyKind::Dynamic(from) => {
                    flux_solve::objects::supertraits_of(self.items, from.trait_)
                        .contains(&object.trait_)
                        && crate::objects::retains_auto_traits(self.sigs, source, target)
                }
                _ => !self.is_unsized(source) && self.may_implement(source, object.trait_),
            };
        }
        if let Some((array, slice)) = crate::shared_pointees(self.items, actual, expected)
            && let (TyKind::Array(elem, _), TyKind::Slice(wanted)) =
                (self.shallow(array).kind(), self.shallow(slice).kind())
        {
            return self.can_unify(elem, wanted);
        }
        if let (Some(from), Some(to)) = (
            crate::nonnull_pointee(self.items, actual),
            crate::nonnull_pointee(self.items, expected),
        ) && let Some(equalities) = self.array_unsize(from, to)
        {
            return equalities.into_iter().all(|(a, b)| self.can_unify(a, b));
        }
        if let (Some(p), Some(q)) = (
            flux_solve::box_pointee(actual),
            flux_solve::box_pointee(expected),
        ) {
            let (p, q) = (self.shallow(p), self.shallow(q));
            if let Some(equalities) = self.array_unsize(p, q) {
                return equalities.into_iter().all(|(a, b)| self.can_unify(a, b));
            }
            if !q.is_dyn()
                && let Some((source, _)) =
                    flux_solve::transparent_object_unsize(self.items, &self.sigs.structs, p, q)
            {
                return !self.sigs.is_unsized(source);
            }
            match (p.kind(), q.kind()) {
                (TyKind::Array(elem, _), TyKind::Slice(wanted)) => {
                    return self.can_unify(elem, wanted);
                }
                (_, TyKind::Dynamic(_)) if !self.can_unify(p, q) => {
                    return !self.sigs.is_unsized(p);
                }
                _ => {}
            }
        }
        if let Some((from, a, to, b)) = match (actual.kind(), expected.kind()) {
            (TyKind::Ref(from, a), TyKind::Ref(to, b))
            | (TyKind::Ref(from, a), TyKind::RawPtr(to, b))
            | (TyKind::RawPtr(from, a), TyKind::RawPtr(to, b)) => Some((from, a, to, b)),
            _ => None,
        } && (from == to || from == Mutability::Mut)
            && let Some(equalities) = self.array_unsize(a, b)
        {
            return equalities.into_iter().all(|(a, b)| self.can_unify(a, b));
        }
        match (actual.kind(), expected.kind()) {
            (TyKind::Ref(from, array), TyKind::Ref(to, slice))
                if (from == to || from == Mutability::Mut)
                    && let (TyKind::Array(elem, _), TyKind::Slice(expected_elem)) =
                        (self.shallow(array).kind(), self.shallow(slice).kind()) =>
            {
                return self.can_unify(elem, expected_elem);
            }
            // To a trait object, as `coerce` unsizes, if the value's type
            // may implement its trait (or is an object of a subtrait).
            (
                TyKind::Ref(from, pointee) | TyKind::RawPtr(from, pointee),
                TyKind::Ref(to, object) | TyKind::RawPtr(to, object),
            ) if (from == to || from == Mutability::Mut)
                && !(matches!(actual.kind(), TyKind::RawPtr(..))
                    && matches!(expected.kind(), TyKind::Ref(..)))
                && let Some((pointee, object)) = flux_solve::transparent_object_unsize(
                    self.items,
                    &self.sigs.structs,
                    self.shallow(pointee),
                    self.shallow(object),
                )
                && let TyKind::Dynamic(d) = object.kind()
                && !self.can_unify(pointee, object) =>
            {
                let pointee = self.shallow(pointee);
                return match pointee.kind() {
                    TyKind::Dynamic(s) => {
                        flux_solve::objects::supertraits_of(self.items, s.trait_)
                            .contains(&d.trait_)
                            && crate::objects::retains_auto_traits(
                                self.sigs,
                                pointee,
                                self.shallow(object),
                            )
                    }
                    _ => !self.is_unsized(pointee) && self.may_implement(pointee, d.trait_),
                };
            }
            (TyKind::Ref(from, pointee), TyKind::Ref(to, expected_pointee))
                if (from == to || from == Mutability::Mut)
                    && !self.can_unify(pointee, expected_pointee) =>
            {
                // A deref coercion, as `coerce` makes it.
                let range = self.range(expr);
                return self
                    .deref_coercion(from, pointee, to, expected_pointee, range)
                    .is_some();
            }
            (TyKind::Ref(Mutability::Mut, pointee), TyKind::Ref(_, expected_pointee)) => {
                return self.can_unify(pointee, expected_pointee);
            }
            (TyKind::Closure(_, args), TyKind::FnPtr(ptr)) => {
                let parts = crate::ClosureArgs::split(args);
                return ptr.abi == flux_hir::Abi::Rust
                    && self.can_unify(parts.params(), Ty::tuple(ptr.params()))
                    && self.can_unify(parts.ret(), ptr.ret());
            }
            (TyKind::FnDef(def), TyKind::FnPtr(ptr)) => {
                if self.items.functions[def.function].intrinsic.is_some() {
                    return false;
                }
                if !ptr.is_unsafe && !self.can_reify_function_features(def.function) {
                    return false;
                }
                let abi = self.items.functions[def.function].abi;
                let reified = Ty::new(TyKind::FnPtr(crate::FnSigTy {
                    is_unsafe: ptr.is_unsafe,
                    ..def.sig
                }));
                return ptr.abi == abi
                    && (ptr.is_unsafe || !def.sig.is_unsafe)
                    && self.can_unify(reified, expected);
            }
            // `fn()` to `unsafe fn()`, as `coerce` makes it.
            (TyKind::FnPtr(from), TyKind::FnPtr(to)) => {
                let offered = Ty::new(TyKind::FnPtr(crate::FnSigTy {
                    is_unsafe: to.is_unsafe,
                    ..from
                }));
                return from.abi == to.abi
                    && (to.is_unsafe || !from.is_unsafe)
                    && self.can_unify(offered, expected);
            }
            (TyKind::Ref(from, pointee), TyKind::RawPtr(to, expected_pointee)) => {
                return (from == to || from == Mutability::Mut)
                    && self.can_unify(pointee, expected_pointee);
            }
            (TyKind::RawPtr(Mutability::Mut, pointee), TyKind::RawPtr(Mutability::Not, wanted)) => {
                return self.can_unify(pointee, wanted);
            }
            _ => {}
        }
        let blamed = self.blame(expr);
        if let Expr::Tuple(fields) = &self.body.exprs[blamed]
            && !self.coerced_tuples.contains(&blamed)
            && let TyKind::Tuple(elements) = expected.kind()
        {
            return fields.len() == elements.len()
                && self.body.lists[*fields]
                    .iter()
                    .zip(elements)
                    .all(|(&field, &element)| {
                        let ty = self.expr_types.get(field).copied().unwrap_or(Ty::ERROR);
                        self.can_coerce(ty, element, field)
                    });
        }
        self.can_unify(actual, expected)
    }

    /// Collect every equality required by structural array unsizing. Probes
    /// only inspect these; coercions must commit prefix and element equalities.
    pub(super) fn array_unsize(&mut self, mut from: Ty, mut to: Ty) -> Option<Vec<(Ty, Ty)>> {
        let mut equalities = Vec::new();
        for _ in 0..256 {
            from = self.shallow(from);
            to = self.shallow(to);
            match (from.kind(), to.kind()) {
                (TyKind::Array(a, _), TyKind::Slice(b)) => {
                    equalities.push((a, b));
                    return Some(equalities);
                }
                (TyKind::Adt(a, _), TyKind::Adt(b, _)) if a == b => {
                    let (a, b) = (
                        flux_solve::fields(self.sigs, from)?,
                        flux_solve::fields(self.sigs, to)?,
                    );
                    let last = a.len().checked_sub(1)?;
                    if a.len() != b.len()
                        || !a[..last]
                            .iter()
                            .zip(&b[..last])
                            .all(|(&a, &b)| self.can_unify(a, b))
                    {
                        return None;
                    }
                    equalities.extend(a[..last].iter().copied().zip(b[..last].iter().copied()));
                    from = a[last];
                    to = b[last];
                }
                _ => return None,
            }
        }
        None
    }
}
