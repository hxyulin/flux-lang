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
    /// A closure's body's requirements of its creator.
    pub requirements: ClosureRequirements,
}

/// A lifetime a closure is given, by where it is: in the reference to
/// itself its method takes (`&self` of `Fn::call`), in what it captures,
/// in its parameters or in its return type, each numbered among those
/// slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ext {
    Static,
    SelfRef,
    Upvar(u32),
    Arg(u32),
    Ret(u32),
}

/// How the lifetimes a closure is given must relate, as its body
/// requires: `a: b` for each `(a, b)`, which its creator checks where
/// it makes the closure (for what it captures) and where it calls it.
#[derive(Debug, Clone, Default)]
pub struct ClosureRequirements {
    pub outlives: Vec<(Ext, Ext, TextRange)>,
}

/// Each closure's requirements, found from its body.
pub type Requirements = FxHashMap<flux_ty::ClosureId, ClosureRequirements>;

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
    requirements: &Requirements,
) -> Regions {
    let mut cx = Cx::new(body, items, sigs, requirements);
    cx.universal_regions();
    cx.generate();
    let points = Points::new(body);
    let liveness = cx.liveness(&points);
    let solution = cx.solve(&points, &liveness);
    let mut errors = cx.check_universal(&solution);
    errors.extend(cx.check_type_tests(&solution));
    errors.append(&mut cx.errors);
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
        requirements: ClosureRequirements {
            outlives: std::mem::take(&mut cx.own_requirements),
        },
    }
}

/// Whether method `method` of `impl` block `imp`, which implements
/// `trait_fn`, asks no more of its lifetimes than the trait's method
/// does, as rustc's `compare_impl_method` checks: calls through the
/// trait assume only the trait's signature. Its parameters must accept
/// what the trait's are given, and what it returns must be what the
/// trait's says, for every choice of the trait method's lifetimes,
/// given what the trait's signature and the block say of them.
pub(crate) fn impl_method_matches(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
    method: flux_hir::FunctionId,
    trait_fn: flux_hir::FunctionId,
) -> bool {
    let (Some(ir), Some(tr), Some(block)) = (
        sigs.regions.fns.get(method),
        sigs.regions.fns.get(trait_fn),
        sigs.regions.impls.get(imp),
    ) else {
        return true;
    };
    let Some(trait_ref) = sigs.impl_traits.get(imp) else {
        return true;
    };
    let body = Body::empty(Some(method));
    let none = Requirements::default();
    let mut cx = Cx::new(&body, items, sigs, &none);
    // The universal regions: `'static`, the block's lifetimes, and the
    // trait method's; the method's own are to be found.
    cx.new_universal("'static".to_owned());
    let owner = ir.owner as usize;
    let block_vars: Vec<Var> = (0..owner)
        .map(|i| cx.new_universal(format!("'{i}")))
        .collect();
    let trait_vars: Vec<Var> = (0..tr.names.len())
        .map(|i| cx.new_universal(format!("'t{i}")))
        .collect();
    let own_vars: Vec<Var> = (owner..ir.names.len()).map(|_| cx.fresh()).collect();
    let map_impl = |r: Region| match r {
        Region::Static => Some(STATIC),
        Region::Param(i) if (i as usize) < owner => block_vars.get(i as usize).copied(),
        Region::Param(i) => own_vars.get(i as usize - owner).copied(),
        Region::Erased => None,
    };
    let map_trait = |r: Region| match r {
        Region::Static => Some(STATIC),
        Region::Param(i) => trait_vars.get(i as usize).copied(),
        Region::Erased => None,
    };
    // The trait's signature for the block's types: `Self` and the
    // trait's parameters are the block's, the method's own parameters
    // the impl method's.
    let impl_params = items.impls[imp].generics.params.len() as u32;
    let mut args: Vec<Ty> = vec![sigs.impls[imp]];
    args.extend(trait_ref.args.iter().copied());
    let own_types = items.functions[method].generics.params.iter();
    args.extend(
        own_types
            .enumerate()
            .map(|(j, p)| Ty::param(impl_params + j as u32, p.name)),
    );
    let mut arg_vars: Vec<Vec<Var>> = Vec::new();
    let header = block
        .self_ty
        .iter()
        .map(|&r| map_impl(r).unwrap_or(STATIC))
        .collect();
    arg_vars.push(header);
    for table in block.trait_args.iter() {
        arg_vars.push(
            table
                .iter()
                .map(|&r| map_impl(r).unwrap_or(STATIC))
                .collect(),
        );
    }
    let (impl_sig, trait_sig) = (&sigs.fns[method], &sigs.fns[trait_fn]);
    let range = TextRange::default();
    let mut map_i = map_impl;
    let mut map_t = map_trait;
    let count = impl_sig.params.len().min(trait_sig.params.len());
    for i in 0..count {
        let ty = cx.normalize(impl_sig.params[i]);
        let mine = cx.instantiate_sig(impl_sig.params[i], &ir.params[i], &mut map_i, &[], &[]);
        let theirs = cx.instantiate_sig(
            trait_sig.params[i],
            &tr.params[i],
            &mut map_t,
            &args,
            &arg_vars,
        );
        match (mine, theirs) {
            (Some(mine), Some(theirs)) if mine.len() == theirs.len() => cx.relate(
                ty,
                &theirs,
                &mine,
                Variance::Covariant,
                Category::Boring,
                range,
            ),
            _ => return false,
        }
    }
    let ty = cx.normalize(impl_sig.ret);
    let mine = cx.instantiate_sig(impl_sig.ret, &ir.ret, &mut map_i, &[], &[]);
    let theirs = cx.instantiate_sig(trait_sig.ret, &tr.ret, &mut map_t, &args, &arg_vars);
    match (mine, theirs) {
        (Some(mine), Some(theirs)) if mine.len() == theirs.len() => cx.relate(
            ty,
            &mine,
            &theirs,
            Variance::Covariant,
            Category::Boring,
            range,
        ),
        _ => return false,
    }
    // What the method assumes must follow.
    for p in ir.outlives.iter() {
        if let Outlives::Region(a, b) = *p
            && let (Some(a), Some(b)) = (map_impl(a), map_impl(b))
        {
            cx.outlives(a, b, Category::Boring, range);
        }
    }
    // What is known: the trait method's bounds and the block's.
    for p in tr.outlives.iter() {
        if let Outlives::Region(a, b) = *p
            && let (Some(a), Some(b)) = (map_trait(a), map_trait(b))
        {
            cx.known_pairs.push((a, b));
        }
    }
    for p in block.outlives.iter() {
        if let Outlives::Region(a, b) = *p
            && let (Some(a), Some(b)) = (map_impl(a), map_impl(b))
        {
            cx.known_pairs.push((a, b));
        }
    }
    cx.close_known();
    // Each region's universal regions, until nothing changes.
    let n = cx.universal.len();
    let mut values: Vec<Vec<bool>> = vec![vec![false; n]; cx.vars as usize];
    for (u, &(var, _)) in cx.universal.iter().enumerate() {
        values[var as usize][u] = true;
    }
    loop {
        let mut changed = false;
        for c in &cx.constraints {
            let sub = values[c.sub as usize].clone();
            for (have, add) in values[c.sup as usize].iter_mut().zip(sub) {
                if add && !*have {
                    *have = true;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    cx.universal.iter().enumerate().all(|(a, &(var, _))| {
        (0..n).all(|b| !values[var as usize][b] || cx.known(a as u32, b as u32))
    })
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
    /// In a closure's body, what each universal region is to its
    /// creator.
    closure_ext: Vec<Ext>,
    /// The requirements of the closures the body makes and calls.
    requirements: &'a Requirements,
    /// In a closure's body, what it requires of its creator.
    own_requirements: Vec<(Ext, Ext, TextRange)>,
    /// Errors found while making constraints.
    errors: Vec<Diagnostic>,
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
    fn new(
        body: &'a Body,
        items: &'a ItemTree,
        sigs: &'a Signatures,
        requirements: &'a Requirements,
    ) -> Cx<'a> {
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
            closure_ext: Vec::new(),
            requirements,
            own_requirements: Vec::new(),
            errors: Vec::new(),
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
        } else if body.owner.is_none() && body.upvars.is_none() {
            // A constant's or static's value lives forever.
            let n = self.local_vars(Body::RETURN).len();
            signature.push((Body::RETURN, Some(vec![STATIC; n])));
        } else if let Some(upvars) = &body.upvars {
            // A closure: each lifetime of what it is given is universal,
            // and how they must relate is its creator's to check.
            let by_ref = upvars.kind != flux_ty::ClosureKind::FnOnce;
            self.closure_ext.push(Ext::Static);
            for i in 0..=body.param_count as u32 {
                let local = Local::from_raw(i);
                for (k, v) in self.local_vars(local).into_iter().enumerate() {
                    let k = k as u32;
                    let ext = match i {
                        0 => Ext::Ret(k),
                        1 if by_ref && k == 0 => Ext::SelfRef,
                        1 if by_ref => Ext::Upvar(k - 1),
                        1 => Ext::Upvar(k),
                        _ => Ext::Arg(k),
                    };
                    self.universal_of.insert(v, self.universal.len() as u32);
                    self.universal
                        .push((v, format!("'{}", self.universal.len())));
                    self.closure_ext.push(ext);
                }
                // What their types imply is known.
                let ty = body.locals[local].ty;
                let vars = self.local_vars(local);
                let mut implied = Vec::new();
                flux_ty::regions::implied_outlives(self.items, sigs, ty, &vars, &mut implied);
                for p in implied {
                    if let Outlives::Region(a, b) = p {
                        self.known_pairs.push((a, b));
                    }
                }
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
        self.close_known();
    }

    /// `known` from `known_pairs`: reflexive, with `'static` outliving
    /// everything, and transitive.
    fn close_known(&mut self) {
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
        if let Some((imp, impl_args)) = flux_ty::traits::select_impl(items, sigs, goal) {
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
            // A raw pointer holds no loan: a reference cast to one relates
            // only what they both point to, and other pointer casts
            // nothing.
            Rvalue::Cast(op, _) if dest_ty.is_pointer() => {
                let (op_ty, vars) = self.operand_regions(op);
                if let (TyKind::Ref(_, a), TyKind::RawPtr(_, b)) = (op_ty.kind(), dest_ty.kind())
                    && a == b
                    && vars.len() == dest_vars.len() + 1
                {
                    self.relate(
                        b,
                        &vars[1..],
                        &dest_vars,
                        Variance::Covariant,
                        category,
                        range,
                    );
                }
            }
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
            Rvalue::Unsize(op, _) | Rvalue::Subslice { slice: op, .. } => {
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
                let TyKind::Closure(id, args) = dest_ty.kind() else {
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
                // What its body requires of what it captures.
                match self.requirements.get(&id) {
                    Some(required) => {
                        for &(a, b, _) in &required.outlives {
                            let map = |e: Ext| match e {
                                Ext::Static => Some(STATIC),
                                Ext::Upvar(k) => dest_vars.get(k as usize).copied(),
                                _ => None,
                            };
                            if let (Some(a), Some(b)) = (map(a), map(b)) {
                                self.outlives(a, b, Category::Assignment, range);
                            }
                        }
                    }
                    None => {
                        // A body not checked: anything it captures may go
                        // into anything else.
                        for pair in dest_vars.windows(2) {
                            self.equate(pair[0], pair[1], Category::Boring, range);
                        }
                    }
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
        // pointers, whose signatures flux does not track lifetimes in:
        // a closure's body says what it requires.
        if crate::fn_method_kind(items, func).is_some()
            && let Some(&self_ty) = generic_args.first()
            && matches!(
                self_ty.kind(),
                TyKind::Closure(..) | TyKind::FnDef(_) | TyKind::FnPtr(_)
            )
        {
            if let TyKind::Closure(id, _) = self_ty.kind()
                && let Some(required) = self.requirements.get(&id)
            {
                return self.closure_call(required, args, dest, range);
            }
            return self.opaque_call(args, dest, range);
        }
        // Closures passed on, which the callee may call.
        for op in args {
            let (ty, vars) = self.operand_regions(op);
            let mut found = Vec::new();
            self.closures_in(ty, &vars, &mut 0, &mut found);
            for (id, upvars) in found {
                self.passed_closure(id, &upvars, args, dest, range);
            }
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

    /// A call of a closure (`Fn::call(&c, (x,))`, `FnMut::call_mut` or
    /// `FnOnce::call_once`) whose body requires `required`: its
    /// lifetimes, mapped to the call's.
    fn closure_call(
        &mut self,
        required: &ClosureRequirements,
        args: &[Operand],
        dest: &Place,
        range: TextRange,
    ) {
        let (Some(closure), Some(params)) = (args.first(), args.get(1)) else {
            return self.opaque_call(args, dest, range);
        };
        let (self_ty, self_vars) = self.operand_regions(closure);
        let (self_ref, upvars) = match self_ty.kind() {
            TyKind::Ref(..) => (
                self_vars.first().copied(),
                self_vars.get(1..).unwrap_or(&[]),
            ),
            _ => (None, &self_vars[..]),
        };
        let upvars = upvars.to_vec();
        let (_, arg_vars) = self.operand_regions(params);
        let (_, dest_vars) = self.place_regions(dest);
        let map = |e: Ext| match e {
            Ext::Static => Some(STATIC),
            Ext::SelfRef => self_ref,
            Ext::Upvar(k) => upvars.get(k as usize).copied(),
            Ext::Arg(k) => arg_vars.get(k as usize).copied(),
            Ext::Ret(k) => dest_vars.get(k as usize).copied(),
        };
        let category = Cx::into(dest);
        for &(a, b, _) in &required.outlives {
            let category = match b {
                Ext::Ret(_) => category,
                _ => Category::CallArgument,
            };
            if let (Some(a), Some(b)) = (map(a), map(b)) {
                self.outlives(a, b, category, range);
            }
        }
    }

    /// The closures in a value of type `ty` with slots `vars`: each one's
    /// id and its captures' slots, into `out`.
    fn closures_in(
        &mut self,
        ty: Ty,
        vars: &[Var],
        next: &mut usize,
        out: &mut Vec<(flux_ty::ClosureId, Vec<Var>)>,
    ) {
        match ty.kind() {
            TyKind::Ref(_, t) => {
                *next += 1;
                self.closures_in(t, vars, next, out);
            }
            TyKind::RawPtr(_, t) | TyKind::Array(t, _) | TyKind::Slice(t) => {
                self.closures_in(t, vars, next, out)
            }
            TyKind::Tuple(list) => {
                for &t in list {
                    self.closures_in(t, vars, next, out);
                }
            }
            TyKind::Adt(adt, list) => {
                *next += self.items.structs[adt.strukt].generics.lifetimes.len();
                for &t in list {
                    self.closures_in(t, vars, next, out);
                }
            }
            TyKind::Closure(id, list) => {
                let n = self.slots(ty) as usize;
                let own = vars.get(*next..*next + n).unwrap_or(&[]).to_vec();
                out.push((id, own));
                let upvars = flux_ty::ClosureArgs::split(list).upvars;
                self.closures_in(upvars, vars, next, out);
            }
            _ => {}
        }
    }

    /// A closure, whose captures have slots `upvars`, passed to a call
    /// that may call it: what its body requires, as far as the call's
    /// lifetimes can say. What it returns may hold anything the call is
    /// given; what it keeps of its parameters, the callee may have
    /// given it from its own locals, which flux cannot tell, so it is
    /// not supported.
    fn passed_closure(
        &mut self,
        id: flux_ty::ClosureId,
        upvars: &[Var],
        args: &[Operand],
        dest: &Place,
        range: TextRange,
    ) {
        let Some(required) = self.requirements.get(&id) else {
            return;
        };
        let mut inputs = Vec::new();
        for op in args {
            inputs.extend(self.operand_regions(op).1);
        }
        let (_, dest_vars) = self.place_regions(dest);
        let category = Cx::into(dest);
        for &(a, b, at) in &required.outlives {
            match (a, b) {
                (Ext::Arg(_), _) => {
                    let message =
                        "closures that keep references to their parameters are not supported yet";
                    let d = Diagnostic::new(at, message);
                    if !self.errors.contains(&d) {
                        self.errors.push(d);
                    }
                }
                (_, Ext::Ret(_)) => {
                    for &d in &dest_vars {
                        for &i in &inputs {
                            self.outlives(i, d, category, range);
                        }
                    }
                }
                (Ext::Upvar(x), Ext::Upvar(y)) => {
                    if let (Some(&x), Some(&y)) = (upvars.get(x as usize), upvars.get(y as usize)) {
                        self.outlives(x, y, Category::CallArgument, range);
                    }
                }
                (Ext::Upvar(x), Ext::Static) => {
                    if let Some(&x) = upvars.get(x as usize) {
                        self.outlives(x, STATIC, Category::CallArgument, range);
                    }
                }
                _ => {}
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
                // A closure's lifetimes are its creator's to relate,
                // except its reference to itself, which nothing it is
                // given may outlive.
                if let Some(upvars) = &self.body.upvars {
                    let (ext_a, ext_b) =
                        (self.closure_ext[a as usize], self.closure_ext[b as usize]);
                    let target = self.universal[b as usize].0;
                    let (_, range) = self.blame(var, target);
                    // A parameter whose written type has a reference has
                    // a lifetime of the closure's own, which what it
                    // returns or keeps may not have.
                    let own_arg = matches!(ext_a, Ext::Arg(_)) && upvars.annotated_refs;
                    if ext_a != Ext::SelfRef && !own_arg {
                        self.own_requirements.push((ext_a, ext_b, range));
                        continue;
                    }
                    let message = if !own_arg && upvars.kind == flux_ty::ClosureKind::FnMut {
                        "captured variable cannot escape `FnMut` closure body"
                    } else {
                        "lifetime may not live long enough"
                    };
                    errors.push(Diagnostic::new(range, message));
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
            .rev()
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
