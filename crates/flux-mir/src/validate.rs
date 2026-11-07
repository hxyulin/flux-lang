//! A check that MIR is well-typed, as rustc's MIR validator does: every
//! assignment's value has the type of the place it is stored in, and
//! every call's arguments and result the types of the callee's
//! parameters and return value. Code generation trusts the types of
//! places, so a mismatch here (a tuple built with the type it had before
//! a coercion of one of its fields, say) is a miscompilation later.
//!
//! [`crate::build_file`] runs it over every body in debug builds, as
//! code generation does over every instance it compiles, and the MIR
//! corpus test over every file of a corpus that builds.

use flux_hir::ItemTree;
use flux_ty::traits::Projection;
use flux_ty::{ClosureOwner, Signatures, Ty, TyKind};

use crate::{
    AggregateKind, BinOp, Body, FileMir, Operand, Place, Rvalue, StatementKind, TerminatorKind,
};

/// The type errors in `body`, as messages naming the statement; `env`
/// says what the associated types of the bounds in scope are.
pub fn validate(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Projection],
    body: &Body,
) -> Vec<String> {
    let mut v = Validator {
        items,
        sigs,
        env,
        body,
        errors: Vec::new(),
    };
    for (bb, data) in body.blocks.iter() {
        for (i, statement) in data.statements.iter().enumerate() {
            if let StatementKind::Assign(place, rvalue) = &statement.kind {
                let at = || format!("bb{}[{i}]", bb.index());
                v.assign(place, rvalue, at);
            }
        }
        v.terminator(&data.terminator.kind, bb.index());
    }
    v.errors
}

/// Panics with the errors [`validate`] finds in `body`, in debug builds.
pub fn assert_valid(
    items: &ItemTree,
    sigs: &Signatures,
    env: &[Projection],
    body: &Body,
    what: &dyn std::fmt::Display,
) {
    if !cfg!(debug_assertions) {
        return;
    }
    let errors = validate(items, sigs, env, body);
    assert!(
        errors.is_empty(),
        "ill-typed MIR in {what}:\n  {}",
        errors.join("\n  ")
    );
}

/// The type errors [`validate`] finds in every body of `mir`: its
/// functions', closures' and constants', each prefixed by what it is
/// the body of.
pub fn validate_file(hir: &flux_hir::Hir, sigs: &Signatures, mir: &FileMir) -> Vec<String> {
    validate_owners(hir, sigs, mir, &|_| true)
}

/// [`validate_file`] of the bodies of the functions and constants
/// `keep` accepts, and of their closures.
pub(crate) fn validate_owners(
    hir: &flux_hir::Hir,
    sigs: &Signatures,
    mir: &FileMir,
    keep: &dyn Fn(ClosureOwner) -> bool,
) -> Vec<String> {
    let items = &hir.items;
    let mut errors = Vec::new();
    for (id, body) in mir.bodies.iter() {
        let owner = ClosureOwner::Function(id);
        if keep(owner) {
            errors.extend(validate_in(items, sigs, owner, false, body));
        }
    }
    for ((c, _), body) in &mir.closures {
        if keep(c.owner) {
            errors.extend(validate_in(items, sigs, c.owner, true, body));
        }
    }
    for (id, body) in mir.consts.iter() {
        let owner = ClosureOwner::Const(id);
        if keep(owner) {
            errors.extend(validate_in(items, sigs, owner, false, body));
        }
    }
    errors
}

/// The type errors [`validate`] finds in `body`, `owner`'s or, if
/// `closure`, a closure's in it, prefixed by what it is the body of.
pub(crate) fn validate_in(
    items: &ItemTree,
    sigs: &Signatures,
    owner: ClosureOwner,
    closure: bool,
    body: &Body,
) -> Vec<String> {
    let (env, name) = match owner {
        ClosureOwner::Function(f) => (
            flux_solve::traits::projections_of(items, sigs, f),
            items.function_path(&items.functions[f]),
        ),
        ClosureOwner::Const(c) if closure => (
            flux_solve::traits::owner_projections(items, sigs, items.consts[c].owner),
            items.const_name(c),
        ),
        ClosureOwner::Const(c) => (Vec::new(), items.const_name(c)),
    };
    let what = if closure {
        format!("a closure in {name}")
    } else {
        name
    };
    let found = validate(items, sigs, &env, body);
    found.into_iter().map(|e| format!("{what}: {e}")).collect()
}

struct Validator<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    env: &'a [Projection],
    body: &'a Body,
    errors: Vec<String>,
}

impl Validator<'_> {
    /// `ty` normalized, its opaque types revealed: a function's body has
    /// its hidden types where its callers have the opaque ones, and an
    /// instance's are revealed.
    fn norm(&self, ty: Ty) -> Ty {
        let ty = flux_solve::traits::normalize(self.items, self.sigs, self.env, ty);
        let ty = flux_solve::traits::reveal(self.sigs, ty);
        flux_solve::traits::normalize(self.items, self.sigs, self.env, ty)
    }

    fn place_ty(&self, place: &Place) -> Ty {
        self.norm(self.body.place_ty(place, self.sigs))
    }

    fn operand_ty(&self, operand: &Operand) -> Ty {
        self.norm(self.body.operand_ty(operand, self.sigs))
    }

    /// Whether a value of type `actual` may be stored where `expected`
    /// is: the same type, or one an error made unknown. An associated
    /// type the bounds in scope do not normalize (an iterator's `Item`
    /// through `IntoIterator::IntoIter`) may be any type here; each
    /// instance's types are known, and checked, in code generation.
    fn same(&self, expected: Ty, actual: Ty) -> bool {
        expected == actual
            || flux_solve::fn_regions::same_shape(expected, actual)
            || has_error(expected)
            || has_error(actual)
            || ((expected.has_aliases() || actual.has_aliases()) && same_shape(expected, actual))
    }

    fn check(&mut self, expected: Ty, actual: Ty, what: impl FnOnce() -> String) {
        if !self.same(expected, actual) {
            self.errors.push(format!(
                "{}: expected `{expected}`, found `{actual}`",
                what()
            ));
        }
    }

    fn assign(&mut self, place: &Place, rvalue: &Rvalue, at: impl Fn() -> String) {
        let ty = self.place_ty(place);
        let what = || format!("{}: `{place:?} = {rvalue:?}`", at());
        match rvalue {
            Rvalue::WithAddr(pointer, address) => {
                assert!(
                    matches!(self.operand_ty(pointer).kind(), TyKind::RawPtr(..)),
                    "{}",
                    what()
                );
                self.check(
                    self.operand_ty(address),
                    Ty::int(flux_ty::IntTy::Usize),
                    what,
                );
                self.check(ty, self.operand_ty(pointer), what);
            }
            Rvalue::OffsetOf(_, _) => self.check(ty, Ty::int(flux_ty::IntTy::Usize), what),
            Rvalue::Use(op) => self.check(ty, self.operand_ty(op), what),
            Rvalue::BinaryOp(op, l, r) => {
                let (l, r) = (self.operand_ty(l), self.operand_ty(r));
                if !matches!(op, BinOp::Shl | BinOp::Shr) {
                    self.check(l, r, || format!("{} (operands)", what()));
                }
                let result = match op {
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Ty::BOOL
                    }
                    _ => l,
                };
                self.check(ty, result, what);
            }
            Rvalue::UnaryOp(_, op) => self.check(ty, self.operand_ty(op), what),
            Rvalue::Cast(_, to) => self.check(ty, self.norm(*to), what),
            Rvalue::InlineAsm {
                inputs, registers, ..
            } => {
                assert_eq!(inputs.len(), registers.len(), "{}", what());
                for (input, register) in inputs.iter().zip(registers) {
                    assert!(
                        flux_ty::inline_asm::supports(
                            self.items,
                            self.sigs,
                            self.operand_ty(input),
                            *register
                        ),
                        "{}",
                        what()
                    );
                }
                let tuple = Ty::tuple(
                    &inputs
                        .iter()
                        .zip(registers.iter())
                        .filter(|(_, register)| !register.input_only())
                        .map(|(op, _)| self.operand_ty(op))
                        .collect::<Vec<_>>(),
                );
                self.check(ty, tuple, what);
            }
            Rvalue::Aggregate(AggregateKind::Tuple, ops) => {
                let elements: Vec<Ty> = ops.iter().map(|op| self.operand_ty(op)).collect();
                self.check(ty, Ty::tuple(&elements), what);
            }
            Rvalue::Aggregate(AggregateKind::Array(elem), ops) => {
                let elem = self.norm(*elem);
                for (i, op) in ops.iter().enumerate() {
                    let found = self.operand_ty(op);
                    self.check(elem, found, || format!("{} (element {i})", what()));
                }
                self.check(ty, Ty::array(elem, ops.len() as u64), what);
            }
            Rvalue::Aggregate(kind, ops) => {
                let (fields, ok) = match (kind, ty.kind()) {
                    (AggregateKind::Union(s, field), TyKind::Adt(adt, _)) => (
                        flux_solve::fields(self.sigs, ty)
                            .and_then(|fs| fs.get(*field as usize..*field as usize + 1)),
                        adt.strukt == *s && self.sigs.unions.contains(s),
                    ),
                    (AggregateKind::Struct(s), TyKind::Adt(adt, _)) => (
                        flux_solve::variant_fields(self.sigs, ty, 0),
                        adt.strukt == *s,
                    ),
                    (AggregateKind::Variant(s, v), TyKind::Adt(adt, _)) => (
                        flux_solve::variant_fields(self.sigs, ty, *v),
                        adt.strukt == *s,
                    ),
                    (AggregateKind::Closure, TyKind::Closure(_, args)) => {
                        (Some(flux_ty::ClosureArgs::split(args).upvar_types()), true)
                    }
                    _ => (None, has_error(ty)),
                };
                if !ok {
                    self.errors
                        .push(format!("{}: an aggregate of another type", what()));
                    return;
                }
                let Some(fields) = fields else { return };
                if fields.len() != ops.len() {
                    self.errors.push(format!(
                        "{}: {} fields, {} operands",
                        what(),
                        fields.len(),
                        ops.len()
                    ));
                    return;
                }
                for (i, (&field, op)) in fields.iter().zip(ops).enumerate() {
                    let found = self.operand_ty(op);
                    self.check(self.norm(field), found, || {
                        format!("{} (field {i})", what())
                    });
                }
            }
            Rvalue::Discriminant(_) | Rvalue::DiscriminantForDrop(_) => {
                if !matches!(ty.kind(), TyKind::Int(_) | TyKind::Error) {
                    self.errors
                        .push(format!("{}: a discriminant stored in `{ty}`", what()));
                }
            }
            Rvalue::Ref {
                raw,
                mutability,
                place,
                ..
            } => {
                let pointee = self.place_ty(place);
                let expected = if *raw {
                    Ty::raw_ptr(*mutability, pointee)
                } else {
                    Ty::new(TyKind::Ref(*mutability, pointee))
                };
                self.check(ty, expected, what);
            }
            Rvalue::Len { .. } => self.check(ty, Ty::int(flux_ty::IntTy::Usize), what),
            Rvalue::Subslice { slice, start, end } => {
                let usize = Ty::int(flux_ty::IntTy::Usize);
                for (bound, op) in [("start", start), ("end", end)] {
                    let found = self.operand_ty(op);
                    self.check(usize, found, || format!("{} ({bound})", what()));
                }
                let from = self.operand_ty(slice);
                let unsized_ref = matches!(from.kind(), TyKind::Ref(_, pointee) | TyKind::RawPtr(_, pointee) if pointee.is_unsized());
                if !unsized_ref && !has_error(from) {
                    self.errors
                        .push(format!("{}: a subslice of a `{from}`", what()));
                }
                self.check(ty, from, what);
            }
            Rvalue::Repeat(op, count) => {
                let elem = self.operand_ty(op);
                self.check(ty, Ty::array_of(elem, *count), what);
            }
            Rvalue::Unsize(op, to) => {
                let (from, to) = (self.operand_ty(op), self.norm(*to));
                let pointees = match (from.kind(), to.kind()) {
                    (TyKind::Ref(m, a), TyKind::Ref(n, b))
                    | (TyKind::RawPtr(m, a), TyKind::RawPtr(n, b))
                        if m == n =>
                    {
                        Some((a, b))
                    }
                    _ => flux_solve::box_pointee(from)
                        .zip(flux_solve::box_pointee(to))
                        .or_else(|| {
                            flux_ty::shared_pointee(self.items, from)
                                .zip(flux_ty::shared_pointee(self.items, to))
                        }),
                };
                let unsizes = pointees.is_some_and(|(a, b)| {
                    let (a, b) =
                        flux_solve::transparent_object_unsize(self.items, &self.sigs.structs, a, b)
                            .unwrap_or((a, b));
                    if let Some((x, y)) = flux_solve::array_unsize(self.sigs, a, b) {
                        return self.same(x, y);
                    }
                    match (a.kind(), b.kind()) {
                        (TyKind::Array(x, _), TyKind::Slice(y)) => self.same(y, x),
                        // Upcasting: to a supertrait's object.
                        (TyKind::Dynamic(x), TyKind::Dynamic(y)) => {
                            flux_solve::objects::supertraits_of(self.items, x.trait_)
                                .contains(&y.trait_)
                                && flux_ty::objects::retains_auto_traits(self.sigs, a, b)
                        }
                        // A sized value, whose trait the type checker proved.
                        (_, TyKind::Dynamic(_)) => !self.sigs.is_unsized(a),
                        _ => false,
                    }
                });
                if !unsizes && !has_error(from) && !has_error(to) {
                    self.errors
                        .push(format!("{}: `{from}` does not unsize to `{to}`", what()));
                }
                self.check(ty, to, what);
            }
        }
    }

    fn terminator(&mut self, kind: &TerminatorKind, bb: usize) {
        match kind {
            TerminatorKind::Call {
                func,
                generic_args,
                args,
                dest,
                target,
            } => {
                let sig = &self.sigs[*func];
                let params: Vec<Ty> = sig
                    .params
                    .iter()
                    .map(|p| self.norm(p.subst(generic_args)))
                    .collect();
                let ret = self.norm(sig.ret.subst(generic_args));
                let variadic = self.items.functions[*func].is_variadic;
                let name = self.items.functions[*func].name;
                self.call(&params, ret, variadic, args, dest, target.is_some(), || {
                    format!("bb{bb}: call of `{name}`")
                });
            }
            TerminatorKind::CallIndirect {
                callee,
                args,
                dest,
                target,
            } => {
                let ty = self.operand_ty(callee);
                let Some(sig) = ty.fn_sig() else {
                    if !has_error(ty) {
                        self.errors
                            .push(format!("bb{bb}: indirect call of a `{ty}`"));
                    }
                    return;
                };
                let params: Vec<Ty> = sig.params().iter().map(|&p| self.norm(p)).collect();
                let ret = self.norm(sig.ret());
                self.call(&params, ret, false, args, dest, target.is_some(), || {
                    format!("bb{bb}: indirect call")
                });
            }
            TerminatorKind::SwitchInt { discr, .. } => {
                let ty = self.operand_ty(discr);
                if !matches!(ty.kind(), TyKind::Int(_) | TyKind::Bool | TyKind::Error) {
                    self.errors.push(format!("bb{bb}: a switch on a `{ty}`"));
                }
            }
            TerminatorKind::Assert { cond, .. } => {
                let ty = self.operand_ty(cond);
                self.check(Ty::BOOL, ty, || format!("bb{bb}: assert"));
            }
            TerminatorKind::Goto(_)
            | TerminatorKind::Drop { .. }
            | TerminatorKind::Return
            | TerminatorKind::Unreachable => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        params: &[Ty],
        ret: Ty,
        variadic: bool,
        args: &[Operand],
        dest: &Place,
        returns: bool,
        what: impl Fn() -> String,
    ) {
        if args.len() < params.len() || (args.len() > params.len() && !variadic) {
            self.errors.push(format!(
                "{}: {} arguments for {} parameters",
                what(),
                args.len(),
                params.len()
            ));
            return;
        }
        for (i, (&param, arg)) in params.iter().zip(args).enumerate() {
            let found = self.operand_ty(arg);
            self.check(param, found, || format!("{} (argument {i})", what()));
        }
        // A call that does not return stores nothing.
        if returns {
            let dest_ty = self.place_ty(dest);
            self.check(dest_ty, ret, || format!("{} (result)", what()));
        }
    }
}

/// Whether `a` and `b` are the same type where each associated type
/// stands for any type.
fn same_shape(a: Ty, b: Ty) -> bool {
    let same = same_shape;
    let all = |x: &[Ty], y: &[Ty]| x.len() == y.len() && x.iter().zip(y).all(|(&x, &y)| same(x, y));
    match (a.kind(), b.kind()) {
        _ if a == b => true,
        (TyKind::Alias(_), _) | (_, TyKind::Alias(_)) => true,
        (TyKind::Tuple(x), TyKind::Tuple(y)) => all(x, y),
        (TyKind::Adt(p, x), TyKind::Adt(q, y)) => p == q && all(x, y),
        (TyKind::Closure(p, x), TyKind::Closure(q, y)) => p == q && all(x, y),
        (TyKind::Ref(m, x), TyKind::Ref(n, y)) | (TyKind::RawPtr(m, x), TyKind::RawPtr(n, y)) => {
            m == n && same(x, y)
        }
        (TyKind::Array(x, m), TyKind::Array(y, n)) => m == n && same(x, y),
        (TyKind::Slice(x), TyKind::Slice(y)) => same(x, y),
        (TyKind::FnPtr(x), TyKind::FnPtr(y)) => {
            x.abi == y.abi && x.is_unsafe == y.is_unsafe && all(x.inputs_output, y.inputs_output)
        }
        (TyKind::FnDef(x), TyKind::FnDef(y)) => x.function == y.function && all(x.args, y.args),
        _ => false,
    }
}

/// Whether `ty` mentions the type of an erroneous expression.
fn has_error(ty: Ty) -> bool {
    match ty.kind() {
        TyKind::Error | TyKind::Infer(_) => true,
        TyKind::Tuple(list) | TyKind::Adt(_, list) | TyKind::Closure(_, list) => {
            list.iter().any(|&t| has_error(t))
        }
        TyKind::Alias(alias) => alias.args.iter().any(|&t| has_error(t)),
        TyKind::RawPtr(_, t) | TyKind::Ref(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
            has_error(t)
        }
        TyKind::FnDef(_) | TyKind::FnPtr(_) => {
            let mut found = false;
            ty.map_fn(|t| {
                found |= has_error(t);
                t
            });
            found
        }
        _ => false,
    }
}
