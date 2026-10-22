//! Region inference, as rustc's NLL borrow checker does it.
//!
//! Every region slot of every local's type gets a *region variable*, and
//! so does each borrow and each lifetime of each call's callee. A region
//! is a set of points (each statement and terminator is one) and of
//! *end markers*, one per universal region: `'static` and the
//! function's lifetime parameters, which hold everywhere in the body
//! and beyond it. Two kinds of constraints give regions their points:
//!
//! - *Liveness*: where a local may still be used (read, or dropped),
//!   every region in its type contains that point.
//! - *Outlives*, `'a: 'b`, from the types of assignments, borrows,
//!   aggregates and calls (subtyping, with each type's variance), from
//!   reborrows (`&*r` needs `r`'s region), and from the callee's bounds
//!   and the outlives requirements its types imply: `'a`'s value
//!   includes `'b`'s.
//!
//! Outlives constraints form a graph whose strongly connected components
//! share their values, computed bottom-up. A universal region whose
//! value has another's end marker must outlive it, which the function's
//! bounds must say ("lifetime may not live long enough"). A loan is in
//! scope from where it is made at every point its region contains that
//! is reachable without leaving the region or passing a write that
//! overwrites what it borrows; [`super`] checks each access against the
//! loans in scope at its point.
//!
//! Signatures' lifetimes come from [`flux_ty::regions`]. A closure's
//! body treats every lifetime of its captures, parameters and return
//! type as one universal region of its own: its creator equates the
//! lifetimes of what it captures, and a call of it lets anything in its
//! inputs flow into its result and into whatever is behind a `&mut` in
//! them. Where a signature's lifetimes cannot be matched to an
//! instance's types (an associated type no `impl` block decides, say),
//! the instance's are fresh regions that everything in the call's
//! inputs outlives.

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::regions::{Outlives, Region, Variance};
use flux_ty::{AliasTy, Mutability, Signatures, Ty, TyKind};
use rustc_hash::FxHashMap;

use crate::bitset::BitSet;
use crate::interval::IntervalSet;
use crate::{
    AggregateKind, BasicBlock, Body, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind,
};

/// A region variable.
pub(crate) type Var = u32;

/// `'static`, always the first.
const STATIC: Var = 0;

/// Why a constraint exists, for choosing what to blame in an error, in
/// rustc's order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Category {
    Return,
    CallArgument,
    Assignment,
    Boring,
}

#[derive(Debug, Clone, Copy)]
struct Constraint {
    sup: Var,
    sub: Var,
    category: Category,
    range: TextRange,
}

/// The points of a body: each block's statements, then its terminator.
pub(crate) struct Points {
    pub start: Vec<u32>,
    pub total: u32,
}

impl Points {
    pub fn new(body: &Body) -> Points {
        let mut start = Vec::with_capacity(body.blocks.len());
        let mut total = 0;
        for data in body.blocks.values() {
            start.push(total);
            total += data.statements.len() as u32 + 1;
        }
        Points { start, total }
    }

    pub fn point(&self, block: BasicBlock, index: usize) -> u32 {
        self.start[block.index()] + index as u32
    }

    /// The point of block `block`'s terminator.
    fn end(&self, block: usize) -> u32 {
        self.start
            .get(block + 1)
            .map_or(self.total - 1, |&next| next - 1)
    }
}

/// What region inference gives the borrow checker.
pub(crate) struct Regions {
    pub points: Points,
    /// The points where each loan is in scope.
    pub loan_scopes: Vec<IntervalSet>,
    /// For each loan whose region must outlive the function (it flows
    /// into what the function returns, or into what a parameter points
    /// to), the constraint blamed: its category and range.
    pub escapes: Vec<Option<(Category, TextRange)>>,
    /// Lifetimes that do not outlive what they must.
    pub errors: Vec<Diagnostic>,
}

/// A loan as region inference needs it.
pub(crate) struct LoanInfo<'a> {
    pub place: &'a Place,
    pub block: BasicBlock,
    pub statement: usize,
}

/// Infers the regions of `body`, whose loans are `loans`, in the order
/// of the statements that make them.
pub(crate) fn infer(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    loans: &[LoanInfo<'_>],
) -> Regions {
    let mut cx = Cx::new(body, items, sigs);
    cx.universal_regions();
    cx.generate();
    let points = Points::new(body);
    let liveness = cx.liveness(&points);
    let solution = cx.solve(&points, &liveness);
    let mut errors = cx.check_universal(&solution);
    errors.extend(cx.check_type_tests(&solution));
    let loan_scopes = cx.loan_scopes(&points, &solution, loans);
    let escapes = (0..loans.len())
        .map(|i| {
            let var = cx.loan_vars[i];
            let value = &solution.values[solution.scc[var as usize] as usize];
            let &target = value.ends.first()?;
            Some(cx.blame(var, cx.universal[target as usize].0))
        })
        .collect();
    Regions {
        points,
        loan_scopes,
        escapes,
        errors,
    }
}

struct Cx<'a> {
    body: &'a Body,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    /// What the bounds in scope say associated types are.
    env: Vec<flux_ty::Projection>,
    slot_counts: FxHashMap<Ty, u32>,
    /// The first variable of each local's slots, and one past the last
    /// local's.
    local_base: Vec<Var>,
    vars: u32,
    /// The universal regions: their variables and names. `'static` is
    /// the first; then the function's lifetime parameters (a closure's
    /// one region for everything it is given); then any made for a
    /// parameter whose signature could not be matched.
    universal: Vec<(Var, String)>,
    /// The universal region each variable is, if any.
    universal_of: FxHashMap<Var, u32>,
    /// `'a: 'b` between universal regions, by index, as the bounds and
    /// signature say.
    known_pairs: Vec<(u32, u32)>,
    /// Their closure: `known[a * n + b]`.
    known: Vec<bool>,
    /// `T: 'a` the bounds and signature say, by universal index.
    known_types: Vec<(Ty, u32)>,
    constraints: Vec<Constraint>,
    /// `T: 'r`: what must hold of a parameter or associated type.
    type_tests: Vec<(Ty, Var, TextRange)>,
    /// Each loan's region, in order.
    loan_vars: Vec<Var>,
    /// Constraints by their `sup`, built for explaining errors.
    by_sup: Vec<Vec<u32>>,
}

/// A region's value: points, and the universal regions it must
/// outlive, by index.
#[derive(Debug, Clone, Default)]
struct Value {
    points: IntervalSet,
    ends: Vec<u32>,
}

struct Solution {
    /// Each variable's strongly connected component.
    scc: Vec<u32>,
    values: Vec<Value>,
}

impl<'a> Cx<'a> {
    fn new(body: &'a Body, items: &'a ItemTree, sigs: &'a Signatures) -> Cx<'a> {
        let env = match body.owner {
            Some(f) => flux_ty::traits::projections_of(items, sigs, f),
            None => Vec::new(),
        };
        Cx {
            body,
            items,
            sigs,
            env,
            slot_counts: FxHashMap::default(),
            local_base: Vec::new(),
            vars: 0,
            universal: Vec::new(),
            universal_of: FxHashMap::default(),
            known_pairs: Vec::new(),
            known: Vec::new(),
            known_types: Vec::new(),
            constraints: Vec::new(),
            type_tests: Vec::new(),
            loan_vars: Vec::new(),
            by_sup: Vec::new(),
        }
    }

    fn slots(&mut self, ty: Ty) -> u32 {
        if let Some(&n) = self.slot_counts.get(&ty) {
            return n;
        }
        let n = flux_ty::regions::slots(self.items, ty) as u32;
        self.slot_counts.insert(ty, n);
        n
    }

    fn fresh(&mut self) -> Var {
        self.vars += 1;
        self.vars - 1
    }

    fn fresh_list(&mut self, n: u32) -> Vec<Var> {
        (0..n).map(|_| self.fresh()).collect()
    }

    fn new_universal(&mut self, name: String) -> Var {
        let var = self.fresh();
        self.universal_of.insert(var, self.universal.len() as u32);
        self.universal.push((var, name));
        var
    }

    fn outlives(&mut self, sup: Var, sub: Var, category: Category, range: TextRange) {
        if sup != sub {
            self.constraints.push(Constraint {
                sup,
                sub,
                category,
                range,
            });
        }
    }

    fn equate(&mut self, a: Var, b: Var, category: Category, range: TextRange) {
        self.outlives(a, b, category, range);
        self.outlives(b, a, category, range);
    }

    fn local_vars(&self, local: Local) -> Vec<Var> {
        let i = local.index();
        (self.local_base[i]..self.local_base[i + 1]).collect()
    }

    fn normalize(&self, ty: Ty) -> Ty {
        flux_ty::traits::normalize(self.items, self.sigs, &self.env, ty)
    }

    /// The universal regions, then each local's slots, and the
    /// constraints that tie the parameters' and return value's slots to
    /// the signature's.
    fn universal_regions(&mut self) {
        let body = self.body;
        let sigs = self.sigs;
        let fn_regions = match (body.owner, &body.upvars) {
            (Some(f), None) => sigs.regions.fns.get(f),
            _ => None,
        };
        self.new_universal("'static".to_owned());
        if let Some(r) = fn_regions {
            for (i, n) in r.names.iter().enumerate() {
                let name = match n.name {
                    Some(name) => name.to_string(),
                    None => format!("'{}", i + 1),
                };
                self.new_universal(name);
            }
        } else if body.upvars.is_some() {
            self.new_universal("'1".to_owned());
        }
        // Locals.
        let mut base = Vec::with_capacity(body.locals.len() + 1);
        for decl in body.locals.values() {
            base.push(self.vars);
            let n = self.slots(decl.ty);
            self.vars += n;
        }
        base.push(self.vars);
        self.local_base = base;
        let range = TextRange::default();
        let mut signature: Vec<(Local, Option<Vec<Var>>)> = Vec::new();
        if let (Some(r), Some(f)) = (fn_regions, body.owner) {
            let map = |region: Region| match region {
                Region::Static => Some(STATIC),
                Region::Param(i) => Some(1 + i),
                Region::Erased => None,
            };
            for p in r.outlives.iter() {
                match *p {
                    Outlives::Region(a, b) => {
                        if let (Some(a), Some(b)) = (map(a), map(b)) {
                            self.known_pairs.push((a, b));
                        }
                    }
                    Outlives::Type(t, b) => {
                        if let Some(b) = map(b) {
                            self.known_types.push((t, b));
                        }
                    }
                }
            }
            let sig = &sigs.fns[f];
            let mut map_var = map;
            for (i, (&ty, regions)) in sig.params.iter().zip(r.params.iter()).enumerate() {
                let vars = self.instantiate_sig(ty, regions, &mut map_var, &[], &[]);
                signature.push((Local::from_raw(i as u32 + 1), vars));
            }
            let vars = self.instantiate_sig(sig.ret, &r.ret, &mut map_var, &[], &[]);
            signature.push((Body::RETURN, vars));
        } else if body.upvars.is_some() {
            // A closure: everything it is given has its one region.
            for i in 0..=body.param_count as u32 {
                let local = Local::from_raw(i);
                let n = self.local_vars(local).len();
                signature.push((local, Some(vec![1; n])));
            }
        }
        for (local, sig_vars) in signature {
            let vars = self.local_vars(local);
            match sig_vars {
                Some(sig_vars) if sig_vars.len() == vars.len() => {
                    for (&v, &s) in vars.iter().zip(&sig_vars) {
                        self.equate(v, s, Category::Boring, range);
                    }
                }
                _ => {
                    // A signature flux could not match to the body's
                    // types: lifetimes of their own, which nothing is
                    // known to outlive.
                    for v in vars {
                        let name = format!("'{}", self.universal.len());
                        let u = self.new_universal(name);
                        self.equate(v, u, Category::Boring, range);
                    }
                }
            }
        }
        // What is known, closed transitively.
        let n = self.universal.len();
        self.known = vec![false; n * n];
        for a in 0..n {
            self.known[a * n + a] = true;
            self.known[a] = true;
        }
        for &(a, b) in &self.known_pairs {
            let (a, b) = (
                self.universal_of[&a] as usize,
                self.universal_of[&b] as usize,
            );
            self.known[a * n + b] = true;
        }
        loop {
            let mut changed = false;
            for a in 0..n {
                for b in 0..n {
                    if !self.known[a * n + b] {
                        continue;
                    }
                    for c in 0..n {
                        if self.known[b * n + c] && !self.known[a * n + c] {
                            self.known[a * n + c] = true;
                            changed = true;
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        if body.upvars.is_some() {
            // How a closure's lifetimes relate is its creator's to check,
            // except that none of them is `'static`.
            for a in 1..n {
                for b in 1..n {
                    self.known[a * n + b] = true;
                }
            }
        }
    }

    /// The slots of an instance of `ty`, a type in a signature's terms
    /// whose own slots are `own`, with `map` giving each of those and
    /// `args`, of slots `arg_vars`, its generic arguments: `None` if it
    /// cannot be matched to the instance's type (an associated type no
    /// `impl` block decides).
    fn instantiate_sig(
        &mut self,
        ty: Ty,
        own: &[Region],
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        let mut out = Vec::new();
        let mut next = 0;
        self.inst(ty, own, &mut next, map, args, arg_vars, &mut out)
            .then_some(out)
    }

    #[allow(clippy::too_many_arguments)]
    fn inst(
        &mut self,
        ty: Ty,
        own: &[Region],
        next: &mut usize,
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        out: &mut Vec<Var>,
    ) -> bool {
        let mut own_slot = |next: &mut usize, out: &mut Vec<Var>| -> bool {
            let r = own.get(*next).copied().unwrap_or(Region::Erased);
            *next += 1;
            match map(r) {
                Some(v) => {
                    out.push(v);
                    true
                }
                None => false,
            }
        };
        match ty.kind() {
            TyKind::Param(p) => {
                if let Some(vars) = arg_vars.get(p.index as usize) {
                    out.extend(vars);
                }
                true
            }
            TyKind::Ref(_, t) => {
                own_slot(next, out) && self.inst(t, own, next, map, args, arg_vars, out)
            }
            TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.inst(t, own, next, map, args, arg_vars, out)
            }
            TyKind::Tuple(list) => list
                .iter()
                .all(|&t| self.inst(t, own, next, map, args, arg_vars, out)),
            TyKind::Adt(adt, list) => {
                let n = self.items.structs[adt.strukt].generics.lifetimes.len();
                (0..n).all(|_| own_slot(next, out))
                    && list
                        .iter()
                        .all(|&t| self.inst(t, own, next, map, args, arg_vars, out))
            }
            TyKind::Closure(_, list) => {
                let upvars = flux_ty::ClosureArgs::split(list).upvars;
                self.inst(upvars, own, next, map, args, arg_vars, out)
            }
            TyKind::Alias(alias) => {
                let instance = if args.is_empty() { ty } else { ty.subst(args) };
                let normalized = self.normalize(instance);
                if self.slots(normalized) == 0 {
                    return true;
                }
                // Each argument's slots, which are its parameters'.
                let mut alias_vars = Vec::new();
                for &a in alias.args {
                    let mut vars = Vec::new();
                    let mut none = |_| None;
                    if !self.inst(a, &[], &mut 0, &mut none, args, arg_vars, &mut vars) {
                        return false;
                    }
                    alias_vars.push(vars);
                }
                let TyKind::Alias(instance) = instance.kind() else {
                    return false;
                };
                match self.normalize_regions(instance, &alias_vars, 0) {
                    Some(vars) => {
                        out.extend(vars);
                        true
                    }
                    None => false,
                }
            }
            _ => true,
        }
    }

    /// The slots of the type `alias`, whose arguments' slots are
    /// `arg_vars`, normalizes to through the `impl` block for it.
    fn normalize_regions(
        &mut self,
        alias: AliasTy,
        arg_vars: &[Vec<Var>],
        depth: u32,
    ) -> Option<Vec<Var>> {
        if depth > 16 {
            return None;
        }
        let (items, sigs) = (self.items, self.sigs);
        let self_ty = alias.args[0];
        let goal = flux_ty::Predicate {
            self_ty,
            trait_ref: flux_ty::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: &alias.args[1..],
            },
        };
        for imp in sigs.impl_index.impls_for(items, alias.trait_, self_ty) {
            let Some(impl_args) = flux_ty::traits::match_impl(items, sigs, imp, goal) else {
                continue;
            };
            let value = flux_ty::traits::impl_type(sigs, imp, alias.name)?;
            let regions = sigs.regions.impls.get(imp)?;
            let mut lifetimes: Vec<Option<Var>> = vec![None; regions.names.len()];
            let mut types: Vec<Option<Vec<Var>>> = vec![None; impl_args.len()];
            let header = sigs.impls[imp];
            let mut matched = self.match_regions(
                header,
                &regions.self_ty,
                self_ty,
                arg_vars.first()?,
                &mut lifetimes,
                &mut types,
            );
            let trait_args = sigs.impl_traits.get(imp).map_or(&[][..], |t| t.args);
            for (k, (&pattern, table)) in
                trait_args.iter().zip(regions.trait_args.iter()).enumerate()
            {
                let (Some(&actual), Some(vars)) = (alias.args.get(k + 1), arg_vars.get(k + 1))
                else {
                    return None;
                };
                matched &=
                    self.match_regions(pattern, table, actual, vars, &mut lifetimes, &mut types);
            }
            if !matched {
                return None;
            }
            let mut type_vars = Vec::with_capacity(types.len());
            for (j, t) in types.into_iter().enumerate() {
                match t {
                    Some(vars) => type_vars.push(vars),
                    None if self.slots(impl_args[j]) == 0 => type_vars.push(Vec::new()),
                    None => return None,
                }
            }
            let table = regions
                .types
                .iter()
                .find(|(n, _)| *n == alias.name)?
                .1
                .clone();
            let mut map = |r: Region| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied().flatten(),
                Region::Erased => None,
            };
            return self.instantiate_sig(value, &table, &mut map, &impl_args, &type_vars);
        }
        None
    }

    /// Matches `pattern`, an `impl` block's header type with slots
    /// `table`, to `actual`, with slots `vars`: binds the block's
    /// lifetimes and its type parameters' slots.
    fn match_regions(
        &mut self,
        pattern: Ty,
        table: &[Region],
        actual: Ty,
        vars: &[Var],
        lifetimes: &mut [Option<Var>],
        types: &mut [Option<Vec<Var>>],
    ) -> bool {
        let (mut t, mut v) = (0, 0);
        self.match_at(
            pattern, table, &mut t, actual, vars, &mut v, lifetimes, types,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn match_at(
        &mut self,
        pattern: Ty,
        table: &[Region],
        t: &mut usize,
        actual: Ty,
        vars: &[Var],
        v: &mut usize,
        lifetimes: &mut [Option<Var>],
        types: &mut [Option<Vec<Var>>],
    ) -> bool {
        let slot = |t: &mut usize, v: &mut usize, lifetimes: &mut [Option<Var>]| -> bool {
            let (r, var) = (table.get(*t).copied(), vars.get(*v).copied());
            *t += 1;
            *v += 1;
            match (r, var) {
                (Some(Region::Param(i)), Some(var)) => match lifetimes.get_mut(i as usize) {
                    Some(slot @ None) => {
                        *slot = Some(var);
                        true
                    }
                    Some(Some(bound)) => *bound == var,
                    None => false,
                },
                (Some(Region::Erased), Some(_)) => true,
                _ => false,
            }
        };
        match (pattern.kind(), actual.kind()) {
            (TyKind::Param(p), _) => {
                let n = self.slots(actual) as usize;
                let Some(own) = vars.get(*v..*v + n) else {
                    return false;
                };
                *v += n;
                match types.get_mut(p.index as usize) {
                    Some(slot @ None) => {
                        *slot = Some(own.to_vec());
                        true
                    }
                    Some(Some(bound)) => bound == own,
                    None => false,
                }
            }
            (TyKind::Ref(_, a), TyKind::Ref(_, b)) => {
                slot(t, v, lifetimes) && self.match_at(a, table, t, b, vars, v, lifetimes, types)
            }
            (TyKind::RawPtr(_, a), TyKind::RawPtr(_, b))
            | (TyKind::Array(a, _), TyKind::Array(b, _))
            | (TyKind::Slice(a), TyKind::Slice(b)) => {
                self.match_at(a, table, t, b, vars, v, lifetimes, types)
            }
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, lifetimes, types)),
            (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) if x.strukt == y.strukt => {
                let n = self.items.structs[x.strukt].generics.lifetimes.len();
                (0..n).all(|_| slot(t, v, lifetimes))
                    && xs
                        .iter()
                        .zip(ys)
                        .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, lifetimes, types))
            }
            _ => {
                // Types without slots of their own to bind.
                let n = self.slots(actual) as usize;
                *v += n;
                self.slots(pattern) == 0 && n == 0
            }
        }
    }

    /// Relates `sub`'s slots to `sup`'s, both of type `ty` (walked):
    /// `sub` is a subtype of `sup`, at variance `variance`.
    fn relate(
        &mut self,
        ty: Ty,
        sub: &[Var],
        sup: &[Var],
        variance: Variance,
        category: Category,
        range: TextRange,
    ) {
        if sub.len() != sup.len() {
            // Types flux cannot match slot by slot: every lifetime of
            // each outlives every one of the other.
            for &a in sub {
                for &b in sup {
                    self.equate(a, b, Category::Boring, range);
                }
            }
            return;
        }
        let mut next = 0;
        self.relate_at(ty, sub, sup, &mut next, variance, category, range);
    }

    #[allow(clippy::too_many_arguments)]
    fn relate_at(
        &mut self,
        ty: Ty,
        sub: &[Var],
        sup: &[Var],
        next: &mut usize,
        variance: Variance,
        category: Category,
        range: TextRange,
    ) {
        let of = |m: Mutability| match m {
            Mutability::Not => Variance::Covariant,
            Mutability::Mut => Variance::Invariant,
        };
        let slot = |cx: &mut Self, next: &mut usize, v: Variance| {
            let (Some(&a), Some(&b)) = (sub.get(*next), sup.get(*next)) else {
                return;
            };
            *next += 1;
            match v {
                Variance::Covariant => cx.outlives(a, b, category, range),
                Variance::Contravariant => cx.outlives(b, a, category, range),
                Variance::Invariant => cx.equate(a, b, category, range),
                Variance::Bivariant => {}
            }
        };
        match ty.kind() {
            TyKind::Ref(m, inner) => {
                slot(self, next, variance);
                self.relate_at(
                    inner,
                    sub,
                    sup,
                    next,
                    variance.xform(of(m)),
                    category,
                    range,
                );
            }
            TyKind::RawPtr(m, inner) => self.relate_at(
                inner,
                sub,
                sup,
                next,
                variance.xform(of(m)),
                category,
                range,
            ),
            TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.relate_at(t, sub, sup, next, variance, category, range)
            }
            TyKind::Tuple(list) => {
                for &t in list {
                    self.relate_at(t, sub, sup, next, variance, category, range);
                }
            }
            TyKind::Adt(adt, args) => {
                let sigs = self.sigs;
                let def = sigs.regions.structs.get(adt.strukt);
                let n = self.items.structs[adt.strukt].generics.lifetimes.len();
                let var = |k: usize| {
                    def.and_then(|d| d.variances.get(k).copied())
                        .unwrap_or(Variance::Invariant)
                };
                for k in 0..n {
                    slot(self, next, variance.xform(var(k)));
                }
                for (j, &t) in args.iter().enumerate() {
                    let v = variance.xform(var(n + j));
                    self.relate_at(t, sub, sup, next, v, category, range);
                }
            }
            TyKind::Closure(_, args) => {
                let upvars = flux_ty::ClosureArgs::split(args).upvars;
                let v = variance.xform(Variance::Invariant);
                self.relate_at(upvars, sub, sup, next, v, category, range);
            }
            _ => {}
        }
    }

    /// The type and slots of `place`.
    fn place_regions(&mut self, place: &Place) -> (Ty, Vec<Var>) {
        let body = self.body;
        let mut ty = body.locals[place.local].ty;
        let mut vars = self.local_vars(place.local);
        let mut variant = None;
        for &elem in place.projection.iter() {
            match (elem, ty.kind()) {
                (ProjectionElem::Deref, TyKind::Ref(_, inner)) => {
                    vars.remove(0);
                    ty = inner;
                }
                (ProjectionElem::Deref, TyKind::RawPtr(_, inner)) => ty = inner,
                (ProjectionElem::Index(_), TyKind::Array(elem, _) | TyKind::Slice(elem)) => {
                    ty = elem
                }
                (ProjectionElem::Downcast(v), _) => {
                    variant = Some(v);
                    continue;
                }
                (ProjectionElem::Field(i), TyKind::Tuple(list)) => {
                    let start: u32 = list[..i as usize].iter().map(|&t| self.slots(t)).sum();
                    let field = list[i as usize];
                    let n = self.slots(field);
                    vars = vars[start as usize..(start + n) as usize].to_vec();
                    ty = field;
                }
                (ProjectionElem::Field(i), TyKind::Closure(_, args)) => {
                    let upvars = flux_ty::ClosureArgs::split(args).upvar_types();
                    let start: u32 = upvars[..i as usize].iter().map(|&t| self.slots(t)).sum();
                    let field = upvars[i as usize];
                    let n = self.slots(field);
                    vars = vars[start as usize..(start + n) as usize].to_vec();
                    ty = field;
                }
                (ProjectionElem::Field(i), TyKind::Adt(..)) => {
                    let (field, field_vars) = self.field_regions(ty, &vars, variant, i);
                    ty = field;
                    vars = field_vars;
                }
                _ => {
                    ty = Ty::ERROR;
                    vars = Vec::new();
                }
            }
            variant = None;
        }
        (ty, vars)
    }

    /// Field `field` (of variant `variant`) of a value of struct or enum
    /// type `ty` with slots `vars`: its type and slots.
    fn field_regions(
        &mut self,
        ty: Ty,
        vars: &[Var],
        variant: Option<u32>,
        field: u32,
    ) -> (Ty, Vec<Var>) {
        let TyKind::Adt(adt, args) = ty.kind() else {
            return (Ty::ERROR, Vec::new());
        };
        let sigs = self.sigs;
        let Some(field_ty) = sigs
            .variant_fields(ty, variant.unwrap_or(0))
            .and_then(|f| f.get(field as usize).copied())
        else {
            return (Ty::ERROR, Vec::new());
        };
        let field_ty = self.normalize(field_ty);
        let start = match sigs.variant_starts.get(adt.strukt) {
            Some(starts) => starts[variant.unwrap_or(0) as usize] as usize,
            None => 0,
        };
        let index = start + field as usize;
        let generic = sigs.structs[adt.strukt].get(index).copied();
        let table = sigs
            .regions
            .structs
            .get(adt.strukt)
            .and_then(|r| r.fields.get(index));
        let want = self.slots(field_ty);
        let lifetimes = self.items.structs[adt.strukt].generics.lifetimes.len();
        if let (Some(generic), Some(table)) = (generic, table) {
            // The arguments' slots, after the lifetimes.
            let mut arg_vars = Vec::with_capacity(args.len());
            let mut at = lifetimes;
            for &a in args {
                let n = self.slots(a) as usize;
                arg_vars.push(vars.get(at..at + n).map_or_else(Vec::new, <[Var]>::to_vec));
                at += n;
            }
            let mut map = |r: Region| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => vars.get(i as usize).copied(),
                Region::Erased => None,
            };
            if let Some(out) =
                self.instantiate_sig(generic, &table.clone(), &mut map, args, &arg_vars)
                && out.len() == want as usize
            {
                return (field_ty, out);
            }
        }
        // A field flux cannot match: as long as all of the value.
        let fresh = self.fresh_list(want);
        for &f in &fresh {
            for &v in vars {
                self.equate(f, v, Category::Boring, TextRange::default());
            }
        }
        (field_ty, fresh)
    }

    fn operand_regions(&mut self, operand: &Operand) -> (Ty, Vec<Var>) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_regions(place),
            // Constants hold no references flux makes, other than to
            // statics and C strings, which are `'static`.
            Operand::Const(c) => {
                let ty = c.ty();
                let n = self.slots(ty);
                (ty, vec![STATIC; n as usize])
            }
        }
    }

    /// The category of a constraint for a value going into `dest`.
    fn into(dest: &Place) -> Category {
        if dest.local == Body::RETURN && dest.projection.is_empty() {
            Category::Return
        } else {
            Category::Assignment
        }
    }

    /// The constraints of every statement and call.
    fn generate(&mut self) {
        let body = self.body;
        for data in body.blocks.values() {
            for statement in &data.statements {
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    self.assign(dest, rvalue, statement.range);
                }
            }
            let range = data.terminator.range;
            match &data.terminator.kind {
                TerminatorKind::Call {
                    func,
                    generic_args,
                    args,
                    dest,
                    ..
                } => self.call(*func, generic_args, args, dest, range),
                TerminatorKind::CallIndirect { args, dest, .. } => {
                    self.opaque_call(args, dest, range)
                }
                _ => {}
            }
        }
    }

    fn assign(&mut self, dest: &Place, rvalue: &Rvalue, range: TextRange) {
        let category = Cx::into(dest);
        let (dest_ty, dest_vars) = self.place_regions(dest);
        match rvalue {
            Rvalue::Use(op) | Rvalue::Cast(op, _) => {
                let (_, vars) = self.operand_regions(op);
                self.relate(
                    dest_ty,
                    &vars,
                    &dest_vars,
                    Variance::Covariant,
                    category,
                    range,
                );
            }
            Rvalue::Unsize(op, _) => {
                let (_, vars) = self.operand_regions(op);
                self.relate(
                    dest_ty,
                    &vars,
                    &dest_vars,
                    Variance::Covariant,
                    category,
                    range,
                );
            }
            Rvalue::Repeat(op, _) => {
                let (_, vars) = self.operand_regions(op);
                let elem = dest_ty.sequence_elem().unwrap_or(Ty::ERROR);
                self.relate(
                    elem,
                    &vars,
                    &dest_vars,
                    Variance::Covariant,
                    category,
                    range,
                );
            }
            Rvalue::Ref {
                mutability, place, ..
            } => {
                let loan = self.fresh();
                self.loan_vars.push(loan);
                let (place_ty, place_vars) = self.place_regions(place);
                if let Some((&first, rest)) = dest_vars.split_first() {
                    self.outlives(loan, first, category, range);
                    let variance = match mutability {
                        Mutability::Not => Variance::Covariant,
                        Mutability::Mut => Variance::Invariant,
                    };
                    self.relate(place_ty, &place_vars, rest, variance, category, range);
                }
                self.reborrow(place, loan, range);
            }
            Rvalue::Aggregate(kind, ops) => {
                self.aggregate(*kind, ops, dest_ty, &dest_vars, category, range)
            }
            Rvalue::BinaryOp(..)
            | Rvalue::UnaryOp(..)
            | Rvalue::Discriminant(_)
            | Rvalue::Len { .. } => {}
        }
    }

    /// `&place` through references: each reference it goes through must
    /// outlive the new one, up to the first shared one, as rustc's
    /// reborrow constraints say.
    fn reborrow(&mut self, place: &Place, loan: Var, range: TextRange) {
        for k in (0..place.projection.len()).rev() {
            if place.projection[k] != ProjectionElem::Deref {
                continue;
            }
            let prefix = Place {
                local: place.local,
                projection: place.projection[..k].into(),
            };
            let (ty, vars) = self.place_regions(&prefix);
            match ty.kind() {
                TyKind::Ref(m, _) => {
                    if let Some(&r) = vars.first() {
                        self.outlives(r, loan, Category::Boring, range);
                    }
                    if m == Mutability::Not {
                        return;
                    }
                }
                _ => return,
            }
        }
    }

    fn aggregate(
        &mut self,
        kind: AggregateKind,
        ops: &[Operand],
        dest_ty: Ty,
        dest_vars: &[Var],
        category: Category,
        range: TextRange,
    ) {
        match kind {
            AggregateKind::Tuple => {
                let TyKind::Tuple(list) = dest_ty.kind() else {
                    return;
                };
                let mut at = 0;
                for (op, &t) in ops.iter().zip(list) {
                    let n = self.slots(t) as usize;
                    let (_, vars) = self.operand_regions(op);
                    let field = dest_vars.get(at..at + n).unwrap_or(&[]).to_vec();
                    self.relate(t, &vars, &field, Variance::Covariant, category, range);
                    at += n;
                }
            }
            AggregateKind::Array(elem) => {
                for op in ops {
                    let (_, vars) = self.operand_regions(op);
                    self.relate(elem, &vars, dest_vars, Variance::Covariant, category, range);
                }
            }
            AggregateKind::Struct(_) | AggregateKind::Variant(..) => {
                let variant = match kind {
                    AggregateKind::Variant(_, v) => Some(v),
                    _ => None,
                };
                for (i, op) in ops.iter().enumerate() {
                    let (field_ty, field) =
                        self.field_regions(dest_ty, dest_vars, variant, i as u32);
                    let (_, vars) = self.operand_regions(op);
                    self.relate(
                        field_ty,
                        &vars,
                        &field,
                        Variance::Covariant,
                        category,
                        range,
                    );
                }
            }
            AggregateKind::Closure => {
                let TyKind::Closure(_, args) = dest_ty.kind() else {
                    return;
                };
                let upvars = flux_ty::ClosureArgs::split(args).upvar_types();
                let mut at = 0;
                for (op, &t) in ops.iter().zip(upvars) {
                    let n = self.slots(t) as usize;
                    let (_, vars) = self.operand_regions(op);
                    let field = dest_vars.get(at..at + n).unwrap_or(&[]).to_vec();
                    self.relate(t, &vars, &field, Variance::Covariant, category, range);
                    at += n;
                }
                // Its body may move anything it captures into anything
                // else it captures.
                for pair in dest_vars.windows(2) {
                    self.equate(pair[0], pair[1], Category::Boring, range);
                }
            }
        }
    }

    fn call(
        &mut self,
        func: flux_hir::FunctionId,
        generic_args: &[Ty],
        args: &[Operand],
        dest: &Place,
        range: TextRange,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        // The `Fn`-family methods of closures, function items and
        // pointers, whose signatures flux does not track lifetimes in.
        if crate::fn_method_kind(items, func).is_some()
            && let Some(&self_ty) = generic_args.first()
            && matches!(
                self_ty.kind(),
                TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_)
            )
        {
            return self.opaque_call(args, dest, range);
        }
        let Some(regions) = sigs.regions.fns.get(func) else {
            return self.opaque_call(args, dest, range);
        };
        let sig = &sigs.fns[func];
        let lifetimes = self.fresh_list(regions.names.len() as u32);
        let mut type_vars = Vec::with_capacity(generic_args.len());
        for &a in generic_args {
            let n = self.slots(a);
            type_vars.push(self.fresh_list(n));
        }
        let mut map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased => None,
        };
        let mut inputs: Vec<Var> = Vec::new();
        let mut unmatched_inputs: Vec<Var> = Vec::new();
        for (i, op) in args.iter().enumerate() {
            let (ty, actual) = self.operand_regions(op);
            inputs.extend(&actual);
            let (Some(&formal_ty), Some(table)) = (sig.params.get(i), regions.params.get(i)) else {
                continue;
            };
            let formal = self.instantiate_sig(formal_ty, table, &mut map, generic_args, &type_vars);
            match formal {
                Some(formal) => self.relate(
                    ty,
                    &actual,
                    &formal,
                    Variance::Covariant,
                    Category::CallArgument,
                    range,
                ),
                None => unmatched_inputs.extend(&actual),
            }
        }
        let category = Cx::into(dest);
        let (dest_ty, dest_vars) = self.place_regions(dest);
        match self.instantiate_sig(sig.ret, &regions.ret, &mut map, generic_args, &type_vars) {
            Some(formal) => self.relate(
                dest_ty,
                &formal,
                &dest_vars,
                Variance::Covariant,
                category,
                range,
            ),
            None => {
                // Whatever the inputs hold.
                for &d in &dest_vars {
                    for &i in &inputs {
                        self.outlives(i, d, category, range);
                    }
                }
            }
        }
        // A parameter flux cannot match may keep any input, and give it
        // to the result.
        for &a in &unmatched_inputs {
            for &b in inputs.iter().chain(&dest_vars) {
                self.equate(a, b, Category::Boring, range);
            }
        }
        // The callee's bounds, and what its types imply.
        for p in regions.outlives.iter() {
            match *p {
                Outlives::Region(a, b) => {
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        self.outlives(a, b, Category::CallArgument, range);
                    }
                }
                Outlives::Type(t, b) => {
                    let Some(b) = map(b) else { continue };
                    if let TyKind::Param(p) = t.kind()
                        && let Some(vars) = type_vars.get(p.index as usize)
                    {
                        for &v in &vars.clone() {
                            self.outlives(v, b, Category::CallArgument, range);
                        }
                    }
                    let instance = self.normalize(t.subst(generic_args));
                    self.type_outlives(instance, b, range);
                }
            }
        }
    }

    /// `T: b` for the parameters and associated types in `ty`.
    fn type_outlives(&mut self, ty: Ty, b: Var, range: TextRange) {
        match ty.kind() {
            TyKind::Param(_) | TyKind::Alias(_) => self.type_tests.push((ty, b, range)),
            TyKind::FnDef(_) | TyKind::FnPtr(_) => {}
            _ => {
                if ty.has_params() || ty.has_aliases() {
                    for t in ty.children() {
                        self.type_outlives(t, b, range);
                    }
                }
            }
        }
    }

    /// A call whose signature's lifetimes flux does not know: anything
    /// in its inputs may go into its result, and into whatever is behind
    /// a `&mut` in them, except a reference into what it points to.
    fn opaque_call(&mut self, args: &[Operand], dest: &Place, range: TextRange) {
        let mut inputs = Vec::new();
        let mut sinks = Vec::new();
        for op in args {
            let (ty, vars) = self.operand_regions(op);
            let mut next = 0;
            self.sinks(ty, &vars, &mut next, false, &mut Vec::new(), &mut sinks);
            inputs.extend(vars);
        }
        let category = Cx::into(dest);
        let (_, dest_vars) = self.place_regions(dest);
        for &d in &dest_vars {
            for &i in &inputs {
                self.outlives(i, d, category, range);
            }
        }
        for (s, enclosing) in &sinks {
            for &i in &inputs {
                if !enclosing.contains(&i) {
                    self.outlives(i, *s, Category::CallArgument, range);
                }
            }
        }
    }

    /// The slots of `ty` (with slots `vars`) behind a `&mut`, into
    /// `out`, each with the references it is behind (`enclosing`).
    fn sinks(
        &mut self,
        ty: Ty,
        vars: &[Var],
        next: &mut usize,
        behind_mut: bool,
        enclosing: &mut Vec<Var>,
        out: &mut Vec<(Var, Vec<Var>)>,
    ) {
        let slot = |next: &mut usize, enclosing: &[Var], out: &mut Vec<(Var, Vec<Var>)>| {
            let v = vars.get(*next).copied();
            if behind_mut && let Some(v) = v {
                out.push((v, enclosing.to_vec()));
            }
            *next += 1;
            v
        };
        match ty.kind() {
            TyKind::Ref(m, inner) => {
                let own = slot(next, enclosing, out);
                let behind = behind_mut || m == Mutability::Mut;
                enclosing.extend(own);
                self.sinks(inner, vars, next, behind, enclosing, out);
                if own.is_some() {
                    enclosing.pop();
                }
            }
            TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.sinks(t, vars, next, behind_mut, enclosing, out)
            }
            TyKind::Tuple(list) => {
                for &t in list {
                    self.sinks(t, vars, next, behind_mut, enclosing, out);
                }
            }
            TyKind::Adt(adt, list) => {
                for _ in 0..self.items.structs[adt.strukt].generics.lifetimes.len() {
                    slot(next, enclosing, out);
                }
                for &t in list {
                    self.sinks(t, vars, next, behind_mut, enclosing, out);
                }
            }
            TyKind::Closure(_, list) => {
                let upvars = flux_ty::ClosureArgs::split(list).upvars;
                self.sinks(upvars, vars, next, behind_mut, enclosing, out);
            }
            _ => {}
        }
    }

    /// The points where each local with slots is live: may be read or
    /// dropped later.
    fn liveness(&mut self, points: &Points) -> Vec<IntervalSet> {
        let body = self.body;
        let tracked: Vec<bool> = (0..body.locals.len())
            .map(|i| self.local_base[i + 1] > self.local_base[i])
            .collect();
        let live_out = super::liveness_where(body, &tracked);
        let mut live = vec![IntervalSet::new(); body.locals.len()];
        // Walking each block backwards: where each live local's current
        // run ends.
        let mut run_end: Vec<Option<u32>> = vec![None; body.locals.len()];
        let mut open: Vec<usize> = Vec::new();
        let mut runs: Vec<Vec<(u32, u32)>> = vec![Vec::new(); body.locals.len()];
        for (block, data) in body.blocks.iter() {
            let end = points.end(block.index());
            for &local in &live_out[block.index()] {
                run_end[local as usize] = Some(end + 1);
                open.push(local as usize);
            }
            let mut at = end;
            let mut set = |local: Local, value: bool, at: u32| {
                let i = local.index();
                if !tracked[i] {
                    return;
                }
                match (value, run_end[i]) {
                    // Defined here: dead before.
                    (false, Some(e)) => {
                        runs[i].push((at + 1, e));
                        run_end[i] = None;
                    }
                    (true, None) => {
                        run_end[i] = Some(at + 1);
                        open.push(i);
                    }
                    _ => {}
                }
            };
            super::terminator_transfer(&data.terminator.kind, &mut |l, v| set(l, v, at));
            for statement in data.statements.iter().rev() {
                at -= 1;
                super::statement_transfer(&statement.kind, &mut |l, v| set(l, v, at));
            }
            let start = points.start[block.index()];
            for i in open.drain(..) {
                if let Some(e) = run_end[i].take() {
                    runs[i].push((start, e));
                }
            }
        }
        for (i, mut list) in runs.into_iter().enumerate() {
            list.sort_unstable();
            for (s, e) in list {
                live[i].push(s, e);
            }
        }
        live
    }

    /// Each region's value: its strongly connected component's, made of
    /// what its members are live at and the values of the components
    /// they outlive.
    fn solve(&mut self, points: &Points, liveness: &[IntervalSet]) -> Solution {
        let n = self.vars as usize;
        // The successors of each variable: what it outlives.
        let mut edges: Vec<Vec<u32>> = vec![Vec::new(); n];
        let mut by_sup: Vec<Vec<u32>> = vec![Vec::new(); n];
        for (i, c) in self.constraints.iter().enumerate() {
            edges[c.sup as usize].push(c.sub);
            by_sup[c.sup as usize].push(i as u32);
        }
        self.by_sup = by_sup;
        let (scc, order) = tarjan(&edges);
        let count = order.len();
        let mut values = vec![Value::default(); count];
        // Each component's own: its locals' liveness and its universal
        // regions'.
        let mut var_local = vec![u32::MAX; n];
        for (local, w) in self.local_base.windows(2).enumerate() {
            for v in w[0]..w[1] {
                var_local[v as usize] = local as u32;
            }
        }
        for v in 0..n {
            let c = scc[v] as usize;
            if let Some(&u) = self.universal_of.get(&(v as u32)) {
                values[c].points.union(&IntervalSet::full(points.total));
                if !values[c].ends.contains(&u) {
                    values[c].ends.push(u);
                }
            }
            let local = var_local[v];
            if local != u32::MAX {
                values[c].points.union(&liveness[local as usize]);
            }
        }
        // The members of each component, for its successors.
        let mut members: Vec<Vec<u32>> = vec![Vec::new(); count];
        for v in 0..n {
            members[scc[v] as usize].push(v as u32);
        }
        // Components come out of Tarjan's algorithm after those they
        // reach.
        for &c in &order {
            let c = c as usize;
            let mut value = std::mem::take(&mut values[c]);
            for &v in &members[c] {
                for &succ in &edges[v as usize] {
                    let s = scc[succ as usize] as usize;
                    if s == c {
                        continue;
                    }
                    value.points.union(&values[s].points);
                    for &e in &values[s].ends {
                        if !value.ends.contains(&e) {
                            value.ends.push(e);
                        }
                    }
                }
            }
            value.ends.sort_unstable();
            values[c] = value;
        }
        Solution { scc, values }
    }

    fn known(&self, a: u32, b: u32) -> bool {
        let n = self.universal.len();
        self.known[a as usize * n + b as usize]
    }

    /// Universal regions that must outlive others the function does not
    /// know they do.
    fn check_universal(&mut self, solution: &Solution) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        for a in 0..self.universal.len() as u32 {
            let var = self.universal[a as usize].0;
            let value = &solution.values[solution.scc[var as usize] as usize];
            for &b in &value.ends.clone() {
                if self.known(a, b) {
                    continue;
                }
                let target = self.universal[b as usize].0;
                let (_, range) = self.blame(var, target);
                errors.push(Diagnostic::new(range, "lifetime may not live long enough"));
            }
        }
        errors
    }

    /// The parameters and associated types that must outlive universal
    /// regions they are not known to.
    fn check_type_tests(&mut self, solution: &Solution) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        if self.body.upvars.is_some() {
            return errors;
        }
        for &(ty, var, range) in &self.type_tests {
            let value = &solution.values[solution.scc[var as usize] as usize];
            for &e in &value.ends {
                let holds = self
                    .known_types
                    .iter()
                    .any(|&(t, b)| t == ty && self.known(self.universal_of[&b], e));
                if holds {
                    continue;
                }
                let what = match ty.kind() {
                    TyKind::Param(_) => format!("the parameter type `{ty}`"),
                    _ => format!("the associated type `{ty}`"),
                };
                let d = Diagnostic::new(range, format!("{what} may not live long enough"));
                if !errors.contains(&d) {
                    errors.push(d);
                }
                break;
            }
        }
        errors
    }

    /// The constraint to blame for `from` outliving `to`: on the path of
    /// constraints between them, the first of the most interesting
    /// category, as rustc picks.
    fn blame(&self, from: Var, to: Var) -> (Category, TextRange) {
        let n = self.vars as usize;
        let mut came: Vec<u32> = vec![u32::MAX; n];
        let mut queue = std::collections::VecDeque::new();
        let mut seen = BitSet::new_empty(n);
        seen.insert(from as usize);
        queue.push_back(from);
        let mut found = false;
        while let Some(v) = queue.pop_front() {
            if v == to {
                found = true;
                break;
            }
            for &c in &self.by_sup[v as usize] {
                let sub = self.constraints[c as usize].sub as usize;
                if !seen.contains(sub) {
                    seen.insert(sub);
                    came[sub] = c;
                    queue.push_back(sub as u32);
                }
            }
        }
        if !found {
            return (Category::Boring, TextRange::default());
        }
        let mut path = Vec::new();
        let mut v = to;
        while v != from {
            let c = came[v as usize];
            path.push(c);
            v = self.constraints[c as usize].sup;
        }
        path.reverse();
        path.iter()
            .map(|&c| &self.constraints[c as usize])
            .filter(|c| c.range != TextRange::default())
            .min_by_key(|c| c.category)
            .map_or((Category::Boring, TextRange::default()), |c| {
                (c.category, c.range)
            })
    }

    /// Where each loan is in scope: from where it is made, the points
    /// its region contains that are reachable without leaving it or
    /// passing a write of what it borrows.
    fn loan_scopes(
        &self,
        points: &Points,
        solution: &Solution,
        loans: &[LoanInfo<'_>],
    ) -> Vec<IntervalSet> {
        let body = self.body;
        // The points that write each local or a part of it, in order.
        let mut kills: Vec<Vec<(u32, &Place)>> = vec![Vec::new(); body.locals.len()];
        let storage_dead: Vec<Place> = (0..body.locals.len() as u32)
            .map(|l| Place::from(Local::from_raw(l)))
            .collect();
        for (block, data) in body.blocks.iter() {
            for (i, statement) in data.statements.iter().enumerate() {
                let point = points.point(block, i);
                match &statement.kind {
                    StatementKind::Assign(dest, _) => kills[dest.local.index()].push((point, dest)),
                    StatementKind::StorageDead(local) => {
                        kills[local.index()].push((point, &storage_dead[local.index()]))
                    }
                    StatementKind::PlaceMention(_) => {}
                }
            }
            if let TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. } =
                &data.terminator.kind
            {
                let point = points.point(block, data.statements.len());
                kills[dest.local.index()].push((point, dest));
            }
        }
        let successors: Vec<Vec<usize>> = body
            .blocks
            .values()
            .map(|d| {
                d.terminator
                    .kind
                    .successors()
                    .iter()
                    .map(|b| b.index())
                    .collect()
            })
            .collect();
        let mut visited = BitSet::new_empty(body.blocks.len());
        let mut touched: Vec<usize> = Vec::new();
        let mut scopes = Vec::with_capacity(loans.len());
        for (id, loan) in loans.iter().enumerate() {
            let var = self.loan_vars[id];
            let value = &solution.values[solution.scc[var as usize] as usize];
            let everywhere = !value.ends.is_empty();
            // A write of the borrowed place, or of a place it is part of,
            // kills the loan.
            let kills = &kills[loan.place.local.index()];
            let kills_loan = |written: &Place| {
                written.projection.len() <= loan.place.projection.len()
                    && written
                        .projection
                        .iter()
                        .zip(loan.place.projection.iter())
                        .all(|(a, b)| a == b)
            };
            let first_kill = |from: u32, to: u32| -> Option<u32> {
                let i = kills.partition_point(|&(p, _)| p < from);
                kills[i..]
                    .iter()
                    .take_while(|&&(p, _)| p <= to)
                    .find(|(_, w)| kills_loan(w))
                    .map(|&(p, _)| p)
            };
            let mut runs: Vec<(u32, u32)> = Vec::new();
            let mut stack: Vec<(usize, u32)> = vec![(
                loan.block.index(),
                points.point(loan.block, loan.statement + 1),
            )];
            while let Some((block, from)) = stack.pop() {
                let end = points.end(block);
                let limit = if everywhere {
                    end
                } else {
                    match value.points.run_of(from) {
                        Some((_, hi)) => (hi - 1).min(end),
                        None => continue,
                    }
                };
                if let Some(k) = first_kill(from, limit) {
                    runs.push((from, k + 1));
                    continue;
                }
                runs.push((from, limit + 1));
                if limit == end {
                    for &succ in &successors[block] {
                        if !visited.contains(succ) {
                            visited.insert(succ);
                            touched.push(succ);
                            stack.push((succ, points.start[succ]));
                        }
                    }
                }
            }
            for b in touched.drain(..) {
                visited.remove(b);
            }
            runs.sort_unstable();
            let mut scope = IntervalSet::new();
            for (s, e) in runs {
                scope.push(s, e);
            }
            scopes.push(scope);
        }
        scopes
    }
}

/// Tarjan's strongly connected components of the graph `edges`,
/// iteratively: each node's component, and the components in the order
/// they complete, each after every one it reaches.
fn tarjan(edges: &[Vec<u32>]) -> (Vec<u32>, Vec<u32>) {
    let n = edges.len();
    const NONE: u32 = u32::MAX;
    let mut index = vec![NONE; n];
    let mut low = vec![0u32; n];
    let mut on_stack = BitSet::new_empty(n);
    let mut stack: Vec<u32> = Vec::new();
    let mut scc = vec![NONE; n];
    let mut order: Vec<u32> = Vec::new();
    let mut next = 0u32;
    // (node, next edge to visit)
    let mut work: Vec<(u32, usize)> = Vec::new();
    for root in 0..n as u32 {
        if index[root as usize] != NONE {
            continue;
        }
        work.push((root, 0));
        while let Some(&(v, i)) = work.last() {
            let vi = v as usize;
            if i == 0 && index[vi] == NONE {
                index[vi] = next;
                low[vi] = next;
                next += 1;
                stack.push(v);
                on_stack.insert(vi);
            }
            if let Some(&w) = edges[vi].get(i) {
                if let Some(top) = work.last_mut() {
                    top.1 += 1;
                }
                let wi = w as usize;
                if index[wi] == NONE {
                    work.push((w, 0));
                } else if on_stack.contains(wi) {
                    low[vi] = low[vi].min(index[wi]);
                }
                continue;
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                let p = parent as usize;
                low[p] = low[p].min(low[vi]);
            }
            if low[vi] == index[vi] {
                let id = order.len() as u32;
                loop {
                    let w = stack.pop().expect("v is on the stack");
                    on_stack.remove(w as usize);
                    scc[w as usize] = id;
                    if w == v {
                        break;
                    }
                }
                order.push(id);
            }
        }
    }
    (scc, order)
}
