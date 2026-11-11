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
//! type as a universal region of its own, and how they must relate are
//! its requirements: of what it captures where it is made, and of its
//! parameters and result where it is called, or where a callee's
//! `Fn`-family bound says how it calls it. Where a signature's
//! lifetimes cannot be matched to an instance's types (an associated
//! type no `impl` block decides, say), the instance's are fresh regions
//! that everything in the call's inputs outlives.

use flux_hir::{Diagnostic, ItemTree};
use flux_syntax::TextRange;
use flux_ty::regions::{Outlives, Region, TraitRefRegions, Variance};
use flux_ty::{AliasTy, Mutability, Signatures, Ty, TyKind};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::bitset::BitSet;
use crate::interval::IntervalSet;
use crate::{
    AggregateKind, BasicBlock, Body, Local, Operand, Place, ProjectionElem, Rvalue, StatementKind,
    TerminatorKind,
};

mod callable;
mod callable_outputs;
mod closure_types;
mod opaque_outputs;
mod ordinary;
mod ranked_projections;
use callable::PointerCheck;
use callable_outputs::ProjectionInputRecipe;
use ordinary::{OrdinaryGoal, OrdinaryWork};
mod rows;
use rows::RegionRow;
mod adt_bounds;
mod associated_types;
mod families;
mod header_patterns;
mod signature_projections;
mod source_signatures;
pub(crate) use signature_projections::check_signature_projection_requirements;
mod structural;
pub(crate) use adt_bounds::check_adt_declarations;
pub(crate) use associated_types::impl_associated_values_match;

/// A region variable.
pub(crate) type Var = u32;

/// Owned temporary slots, so constraint generation can mutate `Cx` while
/// reading them. Most types have at most four regions; larger lists spill.
type Slots = SmallVec<[Var; 4]>;

/// `'static`, always the first.
const STATIC: Var = 0;
// Declaration proofs cross the same nested containers as signature proofs.
const DECLARATION_RECURSION_LIMIT: u32 = 128;

#[derive(Clone, Copy)]
enum UniversalRelationMode {
    Rigid,
    ClosureRequirements,
}

/// Calls prove input well-formedness; reification compares independent
/// binders while retaining the item's explicit and trait obligations.
#[derive(Clone, Copy, PartialEq, Eq)]
enum FunctionUse {
    Call,
    Reification,
    /// Associated Output equality keeps exactly the declared input binders.
    CallableEquality,
}

/// Why a constraint exists, for choosing what to blame in an error, in
/// rustc's order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Category {
    Return,
    /// What an opaque type's hidden type must outlive.
    OpaqueType,
    /// What a closure's body requires, as a bound it is passed for says
    /// it is called: rustc's `from_closure`, whose escaping borrows are
    /// E0597 rather than E0373. Blamed before the call that passes it.
    ClosureBounds,
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
    /// The points where each local that holds references is live.
    pub liveness: Vec<IntervalSet>,
}

/// A lifetime a closure is given, by where it is: in the reference to
/// itself its method takes (`&self` of `Fn::call`), in what it captures,
/// in its parameters or in its return type, each numbered among those
/// slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ext {
    Static,
    /// A named lifetime of the enclosing function, before closure-local slots.
    Parent(flux_hir::FunctionId, u32),
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
    /// Abstract components in the defining parent's generic namespace.
    pub type_outlives: Vec<(Ty, Ext, TextRange)>,
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
    cx.member_constraints();
    let points = Points::new(body);
    let liveness = cx.liveness(&points, true);
    let uses = cx.liveness(&points, false);
    let solution = cx.solve(&points, &liveness, &uses);
    let mut errors = cx.check_universal(&solution);
    // A type that may not live long enough where a lifetime already
    // does not is the same error, which rustc reports once.
    let type_errors = cx.check_type_tests(&solution);
    let regions_at: Vec<TextRange> = errors.iter().map(|d| d.range).collect();
    errors.extend(
        type_errors
            .into_iter()
            .filter(|d| !regions_at.contains(&d.range)),
    );
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
        liveness,
        requirements: ClosureRequirements {
            outlives: std::mem::take(&mut cx.own_requirements),
            type_outlives: std::mem::take(&mut cx.own_type_requirements),
        },
    }
}

/// Split nested opaque outputs at matching return constructors. Region rows
/// include each constructor's own slots before its type argument slots.
type ReturnComponent = (Ty, Vec<Var>, Ty, Vec<Var>);

fn return_components(
    items: &ItemTree,
    required: Ty,
    theirs: &[Var],
    actual: Ty,
    mine: &[Var],
) -> Option<Vec<ReturnComponent>> {
    if matches!(required.kind(), TyKind::Opaque(_)) || !required.has_opaques() {
        return Some(vec![(required, theirs.to_vec(), actual, mine.to_vec())]);
    }
    if !flux_solve::fn_regions::same_shape(
        required.map_children(|_| Ty::ERROR),
        actual.map_children(|_| Ty::ERROR),
    ) {
        return None;
    }
    let own = match required.kind() {
        TyKind::Adt(adt, _) => items.structs[adt.strukt].generics.lifetimes.len(),
        TyKind::Ref(..) => 1,
        TyKind::Tuple(_) | TyKind::Array(..) | TyKind::RawPtr(..) | TyKind::Slice(_) => 0,
        _ => return None,
    };
    let (mut a, mut b) = (own, own);
    let mut out = Vec::new();
    if own != 0 {
        out.push((
            required.map_children(|_| Ty::UNIT),
            theirs.get(..own)?.to_vec(),
            actual.map_children(|_| Ty::UNIT),
            mine.get(..own)?.to_vec(),
        ));
    }
    for (required, actual) in required.children().zip(actual.children()) {
        let n = flux_ty::regions::slots(items, required);
        let m = flux_ty::regions::slots(items, actual);
        out.extend(return_components(
            items,
            required,
            theirs.get(a..a + n)?,
            actual,
            mine.get(b..b + m)?,
        )?);
        a += n;
        b += m;
    }
    (a == theirs.len() && b == mine.len()).then_some(out)
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
    if sigs.impl_traits.get(imp).is_none() {
        return true;
    }
    let Some(selected) = flux_ty::regions::selected_method_signature(items, sigs, imp, method)
    else {
        return false;
    };
    let body = Body::empty(Some(method));
    let none = Requirements::default();
    let mut cx = Cx::new(&body, items, sigs, &none);
    // The universal regions: `'static`, the block's lifetimes, and the
    // trait method's; the method's own are to be found.
    let method_equalities = cx.env.clone();
    let Some((block_vars, header_goals)) = cx.impl_header_environment(imp) else {
        return false;
    };
    cx.env = method_equalities;
    let owner = ir.owner as usize;
    if block_vars.len() != owner {
        return false;
    }
    let trait_owner = tr.owner as usize;
    if block.trait_lifetimes.len() != trait_owner {
        return false;
    }
    let trait_vars: Vec<Var> = (trait_owner..tr.names.len())
        .map(|i| cx.new_universal(format!("'t{i}")))
        .collect();
    let own_vars: Vec<Var> = (owner..ir.names.len()).map(|_| cx.fresh()).collect();
    let map_impl = |r: Region| match r {
        Region::Static => Some(STATIC),
        Region::Param(i) if (i as usize) < owner => block_vars.get(i as usize).copied(),
        Region::Param(i) => own_vars.get(i as usize - owner).copied(),
        Region::Erased | Region::Late(_) => None,
    };
    // The trait's owner parameters are instantiated by the impl header,
    // not fresh universal lifetimes. Only the method's own lifetimes are
    // universal here: `impl<'x> Tr<'x>` maps the trait's `'a` to `'x`.
    let map_trait = |r: Region| match r {
        Region::Static => Some(STATIC),
        Region::Param(i) if (i as usize) < trait_owner => block
            .trait_lifetimes
            .get(i as usize)
            .copied()
            .and_then(map_impl),
        Region::Param(i) => trait_vars.get(i as usize - trait_owner).copied(),
        Region::Erased | Region::Late(_) => None,
    };
    // Explicit method predicates make their referenced lifetimes early-bound.
    // Their generic identity must match before late-bound variance is checked.
    let (Some(impl_early), Some(trait_early)) = (
        flux_ty::regions::method_early_lifetimes(items, sigs, method),
        flux_ty::regions::method_early_lifetimes(items, sigs, trait_fn),
    ) else {
        return false;
    };
    if impl_early.len() != trait_early.len() {
        return false;
    }
    for (impl_index, trait_index) in impl_early.into_iter().zip(trait_early) {
        let (Some(a), Some(b)) = (
            map_impl(Region::Param(impl_index)),
            map_trait(Region::Param(trait_index)),
        ) else {
            return false;
        };
        cx.outlives(a, b, Category::Boring, TextRange::default());
        cx.outlives(b, a, Category::Boring, TextRange::default());
    }
    // What the block's bounds and the method's say associated types are.
    let declared = sigs
        .impl_projections
        .get(imp)
        .map_or(&[][..], |p| p)
        .iter()
        .zip(block.projections.iter())
        .chain(
            sigs.fn_projections
                .get(method)
                .map_or(&[][..], |p| p)
                .iter()
                .zip(ir.projections.iter()),
        );
    for (p, table) in declared {
        let mut map = map_impl;
        if !table.is_empty()
            && let Some(vars) = cx.instantiate_sig(p.ty, table, &mut map, &[], &[])
        {
            cx.env_regions.push((p.alias, vars));
        }
    }
    // The trait's signature for the block's types: `Self` and the
    // trait's parameters are the block's, the method's own parameters
    // the impl method's.
    let impl_params = items.impls[imp].generics.params.len() as u32;
    let mut args: Vec<Ty> = vec![sigs.impls[imp]];
    args.extend(selected.trait_args.iter().map(|arg| arg.ty));
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
    for argument in selected.trait_args.iter() {
        arg_vars.push(
            argument
                .regions
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
    let mut known_implied: Vec<(Var, Var)> = Vec::new();
    for i in 0..count {
        let ty = cx.normalize(selected.params[i].ty);
        let mine = cx.instantiate_sig(
            selected.params[i].ty,
            &selected.params[i].regions,
            &mut map_i,
            &[],
            &[],
        );
        let theirs = cx.instantiate_sig(
            trait_sig.params[i],
            &tr.params[i],
            &mut map_t,
            &args,
            &arg_vars,
        );
        match (mine, theirs) {
            (Some(mine), Some(theirs)) => {
                // The trait's parameter, for the block's types, is well
                // formed: what it implies is known (`&'t0 Guard<'a, T>`
                // that `'a: 't0`), as rustc's `compare_impl_method`
                // assumes.
                let instance = cx.normalize(trait_sig.params[i].subst(&args));
                let mut implied = Vec::new();
                flux_ty::regions::implied_outlives(items, sigs, instance, &theirs, &mut implied);
                for p in implied {
                    match p {
                        Outlives::Region(a, b) => known_implied.push((a, b)),
                        Outlives::Type(ty, region) => cx.known_types.push((ty, region)),
                    }
                }
                cx.known_pairs.extend(known_implied.iter().copied());
                cx.close_known();
                cx.relate_types(
                    instance,
                    &theirs,
                    ty,
                    &mine,
                    Variance::Covariant,
                    Category::Boring,
                    range,
                )
            }
            _ => return false,
        }
    }
    let mine = cx.instantiate_sig(selected.ret.ty, &selected.ret.regions, &mut map_i, &[], &[]);
    let theirs = cx.instantiate_sig(trait_sig.ret, &tr.ret, &mut map_t, &args, &arg_vars);
    match (mine, theirs) {
        (Some(mine), Some(theirs)) => {
            let Some(outputs) = return_components(
                items,
                trait_sig.ret.subst(&args),
                &theirs,
                selected.ret.ty,
                &mine,
            ) else {
                return false;
            };
            for (required_ty, theirs, actual_ty, mine) in outputs {
                if let TyKind::Opaque(required) = required_ty.kind() {
                    // Compare the promised output rows rather than treating the
                    // trait and implementation's independent capture lists as
                    // the same opaque identity.
                    let Some(mut goals) = cx.opaque_bound_goals(required, &theirs) else {
                        return false;
                    };
                    let mut available = header_goals.clone();
                    if let TyKind::Opaque(actual) = actual_ty.kind() {
                        let Some(bounds) = cx.opaque_bound_goals(actual, &mine) else {
                            return false;
                        };
                        available.extend(bounds);
                    }
                    for goal in &mut goals {
                        goal.predicate.self_ty = actual_ty;
                        goal.vars[0] = mine.clone();
                        if !cx.impl_prove_super(goal, &available, 0) {
                            return false;
                        }
                    }
                    for projection in flux_solve::traits::opaque_projections(sigs, required) {
                        let Some(expected) =
                            cx.opaque_projection(required, projection.alias, &theirs, &[])
                        else {
                            return false;
                        };
                        let alias = projection.alias.map_args(|arg| {
                            if arg == Ty::opaque(required) {
                                actual_ty
                            } else {
                                arg
                            }
                        });
                        // The bound's non-self type arguments retain their exact
                        // substituted rows. Inputs beyond self need represented
                        // rows; do not invent them for a lifetime-bearing type.
                        let mut rows = vec![mine.clone()];
                        for &argument in alias.args.iter().skip(1) {
                            if cx.slots(argument) != 0 {
                                return false;
                            }
                            rows.push(Vec::new());
                        }
                        let Some(actual) = cx.normalize_regions(alias, &rows, 0) else {
                            return false;
                        };
                        if actual.len() != expected.len() {
                            return false;
                        }
                        for (actual, expected) in actual.into_iter().zip(expected) {
                            cx.outlives(actual, expected, Category::Boring, range);
                            cx.outlives(expected, actual, Category::Boring, range);
                        }
                    }
                    let Some(decl) = tr.opaques.get(required.index as usize) else {
                        return false;
                    };
                    for &bound in &decl.outlives {
                        let Some(bound) = map_trait(bound) else {
                            return false;
                        };
                        for &capture in &mine {
                            cx.outlives(capture, bound, Category::Boring, range);
                        }
                    }
                } else {
                    cx.relate_types(
                        cx.normalize(actual_ty),
                        &mine,
                        cx.normalize(required_ty),
                        &theirs,
                        Variance::Covariant,
                        Category::Boring,
                        range,
                    );
                }
            }
        }
        _ => return false,
    }
    // What the method assumes must follow.
    for p in ir.outlives.iter() {
        match *p {
            Outlives::Region(a, b) => {
                if let (Some(a), Some(b)) = (map_impl(a), map_impl(b)) {
                    cx.outlives(a, b, Category::Boring, range);
                }
            }
            Outlives::Type(ty, region) => {
                // Explicit outlives clauses on captured projections retain
                // their signature gate. Such entries here are implied by
                // parameter formation, whose occurrence rows were compared
                // above; an erased Ty cannot represent that input domain.
                if matches!(ty.kind(), TyKind::Alias(alias)
                    if flux_ty::regions::has_projection_inputs(items, alias))
                {
                    continue;
                }
                let Some(region) = map_impl(region) else {
                    return false;
                };
                cx.type_outlives(cx.normalize(ty), region, range);
            }
        }
    }
    // What is known: the trait method's bounds and the block's.
    for p in tr.outlives.iter() {
        match *p {
            Outlives::Region(a, b) => {
                if let (Some(a), Some(b)) = (map_trait(a), map_trait(b)) {
                    cx.known_pairs.push((a, b));
                }
            }
            Outlives::Type(ty, region) => {
                let Some(region) = map_trait(region) else {
                    return false;
                };
                // A trait parameter can become a compound type in the impl
                // header. Preserve its bound on each outlives component,
                // just as type_outlives decomposes the matching obligation.
                let ty = cx.normalize(ty.subst(&args));
                for component in flux_ty::regions::outlives_type_components(sigs, ty) {
                    cx.known_types.push((component, region));
                }
            }
        }
    }
    for p in block.outlives.iter() {
        if let Outlives::Region(a, b) = *p
            && let (Some(a), Some(b)) = (map_impl(a), map_impl(b))
        {
            cx.known_pairs.push((a, b));
        }
    }
    cx.known_pairs.extend(known_implied);
    cx.close_known();
    let region_dependent_bounds = sigs
        .fn_predicates
        .get(method)
        .into_iter()
        .flatten()
        .any(|&predicate| cx.requires_lifetime_proof(predicate, &mut FxHashSet::default(), 0));
    if region_dependent_bounds
        || ir
            .bound_traits
            .iter()
            .chain(tr.bound_traits.iter())
            .any(|bound| bound.bound != 0)
    {
        let impl_lifetimes = (0..ir.names.len())
            .map(|i| map_impl(Region::Param(i as u32)))
            .collect::<Option<Vec<_>>>();
        let trait_lifetimes = (0..tr.names.len())
            .map(|i| map_trait(Region::Param(i as u32)))
            .collect::<Option<Vec<_>>>();
        let (Some(impl_lifetimes), Some(trait_lifetimes)) = (impl_lifetimes, trait_lifetimes)
        else {
            return false;
        };
        let mut type_rows = arg_vars.clone();
        for &ty in args.iter().skip(type_rows.len()) {
            let count = cx.slots(ty);
            type_rows.push(cx.fresh_list(count));
        }
        let mut env = Vec::new();
        for (predicates, rows, bounds, life, substitution, type_vars) in [
            (
                sigs.fn_predicates.get(trait_fn).map_or(&[][..], |p| p),
                &tr.bound_selfs,
                &tr.bound_traits,
                &trait_lifetimes[..],
                &args[..],
                &type_rows[..],
            ),
            (
                sigs.impl_predicates.get(imp).map_or(&[][..], |p| p),
                &block.bound_selfs,
                &block.bound_traits,
                &impl_lifetimes[..owner],
                &[][..],
                &[][..],
            ),
        ] {
            for ((&predicate, row), bound) in predicates.iter().zip(rows.iter()).zip(bounds.iter())
            {
                let Some(goal) = cx.impl_region_goal_with_proof(
                    predicate,
                    row,
                    bound,
                    life,
                    substitution,
                    type_vars,
                    Some((&header_goals, 0)),
                    false,
                ) else {
                    return false;
                };
                if !cx.impl_super_goals(goal, &mut env, 0) {
                    return false;
                }
            }
        }
        for ((&predicate, row), bound) in sigs
            .fn_predicates
            .get(method)
            .into_iter()
            .flatten()
            .zip(ir.bound_selfs.iter())
            .zip(ir.bound_traits.iter())
        {
            let Some(goal) = cx.impl_region_goal_with_proof(
                predicate,
                row,
                bound,
                &impl_lifetimes,
                &[],
                &[],
                Some((&env, 0)),
                false,
            ) else {
                return false;
            };
            if !cx.impl_prove_super(&goal, &env, 0) {
                return false;
            }
        }
        cx.close_known();
    }
    // A normalized comparison row is usable only after proving the original
    // occurrence's selected impl from the original header assumptions. Never
    // treat the selection's lifetime requirements as additional assumptions.
    for (alias, table) in &selected.selections {
        let mut at = 0;
        let mut vars = Vec::new();
        for &ty in alias.args {
            let count = flux_ty::regions::slots(items, ty);
            let Some(row) = table.get(at..at + count) else {
                return false;
            };
            let Some(row) = cx.instantiate_sig(ty, row, &mut { map_impl }, &[], &[]) else {
                return false;
            };
            vars.push(row);
            at += count;
        }
        if at != table.len() {
            return false;
        }
        let goal = ImplRegionGoal {
            predicate: flux_ty::Predicate {
                self_ty: alias.args[0],
                trait_ref: flux_ty::TraitRef {
                    trait_: alias.trait_,
                    name: alias.trait_name,
                    args: Ty::list(&alias.args[1..]),
                },
            },
            vars,
            lifetimes: Vec::new(),
            bound: Vec::new(),
        };
        if !cx.impl_prove_super(&goal, &header_goals, 0) {
            return false;
        }
    }
    cx.close_known();
    let solution = cx.solve(&Points::new(&body), &[], &[]);
    cx.errors.is_empty()
        && cx.check_universal(&solution).is_empty()
        && cx.check_type_tests(&solution).is_empty()
}

/// Checks the supertraits at the declaration, even for an unused, empty
/// impl. Header lifetimes are universal: selecting a supertrait impl may
/// require relations, but may not add them to what the header guarantees.
pub(crate) fn impl_supertraits_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
) -> Option<bool> {
    impl_header_match(items, sigs, imp, false)
}

/// Associated-value bounds use the same universal header assumptions as
/// supertraits, but remain a separate set of declaration obligations.
pub(crate) fn impl_associated_bounds_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
) -> Option<bool> {
    impl_header_match(items, sigs, imp, true)
}

fn impl_header_match(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
    associated: bool,
) -> Option<bool> {
    let (Some(block), Some(&self_ty), Some(&trait_ref)) = (
        sigs.regions.impls.get(imp),
        sigs.impls.get(imp),
        sigs.impl_traits.get(imp),
    ) else {
        return Some(true);
    };
    if sigs.traits.get(trait_ref.trait_).is_none_or(|t| {
        if associated {
            t.type_bounds.is_empty()
        } else {
            t.supertraits.is_empty()
                && sigs
                    .trait_predicates
                    .get(trait_ref.trait_)
                    .is_none_or(|p| p.is_empty())
        }
    }) {
        return Some(true);
    }
    let body = Body::empty(None);
    let none = Requirements::default();
    let mut cx = Cx::new(&body, items, sigs, &none);
    cx.env = sigs
        .impl_projections
        .get(imp)
        .map_or(Vec::new(), |p| p.to_vec());
    cx.header_predicates = sigs
        .impl_predicates
        .get(imp)
        .map_or(Vec::new(), |predicates| {
            flux_solve::traits::elaborate(sigs, predicates)
        });
    cx.unsized_params = flux_solve::unsized_params(
        items,
        Some(flux_hir::Owner::Impl(imp)),
        &flux_hir::Generics::default(),
    );
    cx.header_type_equalities();
    cx.new_universal("'static".into());
    let lifetimes: Vec<_> = (0..block.names.len())
        .map(|i| cx.new_universal(format!("'{i}")))
        .collect();
    let map = |r| match r {
        Region::Static => Some(STATIC),
        Region::Param(i) => lifetimes.get(i as usize).copied(),
        Region::Erased | Region::Late(_) => None,
    };
    for p in block.outlives.iter() {
        match *p {
            Outlives::Region(a, b) => {
                let (Some(a), Some(b)) = (map(a), map(b)) else {
                    return None;
                };
                cx.known_pairs.push((a, b));
            }
            Outlives::Type(t, b) => {
                let b = map(b)?;
                cx.known_types.push((t, b));
            }
        }
    }
    let written = sigs
        .impl_predicates
        .get(imp)
        .into_iter()
        .flatten()
        .copied()
        .enumerate()
        .map(|(index, predicate)| {
            (
                predicate,
                block.bound_selfs.get(index).cloned().unwrap_or_default(),
                block.bound_traits.get(index).cloned().unwrap_or_default(),
            )
        })
        .collect();
    let mut env = cx.original_region_goals(written, &lifetimes);
    // Projection instantiation in this declaration has no body owner from
    // which to recover assumptions. Retain only the original impl header.
    cx.family_goals = env.clone();
    for (p, table) in cx.env.clone().iter().zip(block.projections.iter()) {
        if let Some(vars) = cx.instantiate_sig(p.ty, table, &mut { map }, &[], &[]) {
            cx.env_regions.push((p.alias, vars));
        }
    }
    let vars = cx.instantiate_sig(self_ty, &block.self_ty, &mut { map }, &[], &[])?;
    let mut root = ImplRegionGoal {
        lifetimes: block
            .trait_lifetimes
            .iter()
            .map(|&r| map(r))
            .collect::<Option<_>>()?,
        predicate: trait_ref.with_self(self_ty),
        vars: vec![vars],
        bound: Vec::new(),
    };
    for (&ty, table) in trait_ref.args.iter().zip(block.trait_args.iter()) {
        let vars = cx.instantiate_sig(ty, table, &mut { map }, &[], &[])?;
        root.vars.push(vars);
    }
    let mut goals = Vec::new();
    let args: Vec<_> = std::iter::once(self_ty)
        .chain(trait_ref.args.iter().copied())
        .collect();
    // Definitions are substitutions, not the required equalities. Their
    // projection-selection well-formedness is a separate declaration check.
    for &(name, ref table) in &block.types {
        if !items.traits[trait_ref.trait_].generics.lifetimes.is_empty()
            || items.impls[imp].types.iter().any(|t| {
                t.name == name
                    && (!t.generics.lifetimes.is_empty() || !t.generics.params.is_empty())
            })
        {
            continue;
        }
        let value = flux_solve::traits::impl_type(sigs, imp, name)?;
        // An abstract projection guaranteed by an original header bound
        // carries that declaration's bounds. It selects no concrete impl.
        if let TyKind::Alias(alias) = value.kind()
            && !flux_ty::regions::has_projection_inputs(items, alias)
            && cx.normalize(value) == value
            && env.iter().any(|g| {
                g.predicate.self_ty == alias.args[0]
                    && g.predicate.trait_ref.trait_ == alias.trait_
                    && g.predicate.trait_ref.args == &alias.args[1..]
            })
        {
            let header: Vec<_> = env.iter().map(|g| g.predicate).collect();
            cx.header_aliases.push(alias);
            for bound in flux_solve::traits::alias_bounds(sigs, &header, value) {
                let goal =
                    cx.impl_region_goal(bound, &[], &TraitRefRegions::default(), &[], &[], &[])?;
                if !cx.impl_super_goals(goal, &mut env, 0) {
                    return None;
                }
            }
        }
        if !cx.impl_aliases_known(value, &[]) && !cx.impl_aliases_proven(value, &[], &env, 0) {
            let selections = block
                .type_selections
                .iter()
                .find(|(known, _)| *known == name)
                .map_or(&[][..], |(_, selections)| selections);
            if !cx.associated_value_selections(
                value,
                selections,
                &lifetimes,
                &env,
                &mut FxHashMap::default(),
                0,
            ) {
                return None;
            }
        }
        let vars = cx.instantiate_sig(value, table, &mut { map }, &[], &[])?;
        if vars.len() != cx.slots(cx.normalize(value)) as usize {
            return None;
        }
        let alias = flux_solve::traits::alias_of(items, trait_ref, self_ty, name);
        cx.env.push(flux_ty::Projection { alias, ty: value });
        cx.env_regions.push((alias, vars));
    }
    if associated {
        let sig = sigs.traits.get(trait_ref.trait_)?;
        for &(name, bound) in &sig.type_bounds {
            if !items.traits[trait_ref.trait_].generics.lifetimes.is_empty()
                || items.impls[imp].types.iter().any(|t| {
                    t.name == name
                        && (!t.generics.lifetimes.is_empty() || !t.generics.params.is_empty())
                })
            {
                continue;
            }
            let alias = AliasTy {
                args: Ty::list(
                    &(0..args.len())
                        .map(|i| Ty::param(i as u32, flux_intern::kw::SelfUpper))
                        .collect::<Vec<_>>(),
                ),
                ..flux_solve::traits::alias_of(items, trait_ref, self_ty, name)
            };
            let goal = cx.impl_region_goal(
                bound.with_self(Ty::alias(alias)),
                &[],
                &TraitRefRegions::default(),
                &[],
                &args,
                &root.vars,
            )?;
            if !cx.impl_super_goals_with_inputs(goal, &mut goals, 0, Some(&env)) {
                return None;
            }
        }
        for goal in &goals {
            if !cx.impl_prove_super(goal, &env, 0) {
                return None;
            }
            if !cx.impl_super_equalities(goal, &env)? {
                return Some(false);
            }
        }
        if !cx.impl_equalities(&root, &env, true)? {
            return Some(false);
        }
    } else {
        if !cx.trait_parameters_hold(&root, &env, 0)? {
            return Some(false);
        }
        if !cx.impl_super_goals_with_inputs(root, &mut goals, 0, Some(&env)) {
            return None;
        }
        for goal in goals.iter().skip(1) {
            if !cx.impl_prove_super(goal, &env, 0) {
                return None;
            }
        }
        for goal in &goals {
            if !cx.impl_super_equalities(goal, &env)? {
                return Some(false);
            }
        }
    }
    cx.close_known();
    let solution = cx.solve(&Points::new(&body), &[], &[]);
    Some(
        cx.check_universal(&solution).is_empty()
            && cx.check_type_tests(&solution).is_empty()
            && cx.errors.is_empty(),
    )
}

/// The erased predicate and its separate region slots. Trait declarations
/// with lifetime parameters remain guarded by signature checking.
#[derive(Clone, PartialEq, Eq)]
struct ImplRegionGoal {
    lifetimes: Vec<Var>,
    predicate: flux_ty::Predicate,
    vars: Vec<Vec<Var>>,
    /// Universally quantified callback inputs. An environment assumption may
    /// instantiate these; a required goal must hold for their arbitrary values.
    bound: Vec<Var>,
}

struct Cx<'a> {
    body: &'a Body,
    items: &'a ItemTree,
    sigs: &'a Signatures,
    /// What the bounds in scope say associated types are.
    env: Vec<flux_ty::Projection>,
    /// Abstract associated values whose owner is an original impl-header
    /// predicate. Only the associated-bound declaration check populates this.
    header_aliases: Vec<AliasTy>,
    family_depth: u32,
    family_goals: Vec<ImplRegionGoal>,
    /// Original caller assumptions already reconstructed during projection re-entry.
    caller_goals_in_progress: Option<Vec<ImplRegionGoal>>,
    /// Declaration proof cycles are coinductive only across Send/Sync goals
    /// with identical types and occurrence rows. Ordinary edges stay inductive.
    declaration_stack: Vec<(ImplRegionGoal, bool)>,
    header_predicates: Vec<flux_ty::Predicate>,
    unsized_params: Vec<bool>,
    slot_counts: FxHashMap<Ty, u32>,
    selected_source_signatures: FxHashMap<
        flux_hir::FunctionId,
        Option<std::rc::Rc<flux_ty::regions::SelectedMethodSignature>>,
    >,
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
    /// The function's universal regions that its parameters' types
    /// name, by index.
    in_params: Vec<u32>,
    /// What the bounds in scope say associated types are, where they
    /// name lifetimes (`I: Iterator<Item = &'a T>`): their slots.
    env_regions: Vec<(AliasTy, Vec<Var>)>,
    /// The category of the constraints proving a bound makes: a call's
    /// argument's, or, for a copy's `Copy`, none in particular.
    proving: Category,
    constraints: Vec<Constraint>,
    /// `T: 'r`: what must hold of a parameter or associated type.
    type_tests: Vec<(Ty, Var, TextRange)>,
    /// Each loan's region, in order.
    loan_vars: Vec<Var>,
    /// Constraints by their `sup`, built for explaining errors.
    by_sup: Adjacency,
    /// In a closure's body, what each universal region is to its
    /// creator.
    closure_ext: Vec<Ext>,
    /// The requirements of the closures the body makes and calls.
    requirements: &'a Requirements,
    /// In a closure's body, what it requires of its creator.
    own_requirements: Vec<(Ext, Ext, TextRange)>,
    own_type_requirements: Vec<(Ty, Ext, TextRange)>,
    /// Errors found while making constraints.
    errors: Vec<Diagnostic>,
    /// In a function returning opaque types, the slots of their hidden
    /// types in its return value.
    hidden: Vec<HiddenSlots>,
    /// The variables made for lifetimes elided in bounds: in the
    /// parameters of a `Fn`-family bound, its own (higher-ranked).
    holes: rustc_hash::FxHashSet<Var>,
    /// Relations implied by well-formed higher-ranked input types,
    /// such as the object bound outliving the reference in `&dyn Tr`.
    hole_pairs: rustc_hash::FxHashSet<(Var, Var)>,
    /// Pointer callback proofs wait until output projections have added their
    /// links, so a higher-ranked input cannot escape through Output.
    pointer_checks: Vec<PointerCheck>,
    /// The slots of the inputs of the call whose bounds are being
    /// proven: what the callee may give a closure where its parameters'
    /// slots cannot be told.
    call_inputs: Vec<Var>,
    /// The closures whose `Fn`-family bounds the call being checked
    /// proved, and those whose `Output` it normalized: whose
    /// requirements of their parameters' and return value's lifetimes
    /// it related to its own.
    proven_closures: rustc_hash::FxHashSet<flux_ty::ClosureId>,
    output_closures: rustc_hash::FxHashSet<flux_ty::ClosureId>,
    /// Where the call being checked is.
    call_range: TextRange,
    /// What the function's own `Fn`-family bounds say their results are,
    /// where those have the bounds' own lifetimes (`F: Fn(&T) -> &U`):
    /// at each call, the argument's (`Region::Late`).
    env_late: Vec<(AliasTy, Box<[Region]>)>,
    env_ranked: Vec<ranked_projections::RankedProjection>,
    /// The slots of the parameters of the `Fn`-family bound being
    /// proven, which its result's `Region::Late` lifetimes are.
    late_inputs: Vec<Var>,
    /// What the bounds of the call being checked say associated types
    /// are, with the tables of their slots.
    call_projections: Vec<(flux_ty::traits::Projection, Box<[Region]>)>,
}

/// What a lifetime a closure is given is, where a bound says what it is
/// called with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ClosureSlot {
    Var(Var),
    /// The bound's own lifetime (`&T` in `F: Fn(&T)`), a different one
    /// at each call: nothing else may outlive it, nor it anything else.
    HigherRanked(Var),
    /// Something the call is given, which flux cannot tell.
    Input,
    /// Related where the closure's `Output` is normalized.
    Ret,
    None,
}

/// The slots of an opaque type's hidden type in its function's return
/// value, and the universal regions it must outlive one of (those it
/// captures) or all of (its `+ 'a` bounds).
struct HiddenSlots {
    opaque: flux_ty::OpaqueTy,
    ty: Ty,
    vars: Vec<Var>,
    captures: Vec<Var>,
    outlives: Vec<Var>,
    range: TextRange,
    name: flux_intern::Symbol,
}

/// A region's value: points, and the universal regions it must
/// outlive, by index.
#[derive(Debug, Clone, Default)]
struct Value {
    points: IntervalSet,
    ends: Vec<u32>,
}

/// What matching an `impl` block's header binds: each of its lifetimes,
/// and each of its type parameters' slots.
struct ImplBinding {
    lifetimes: Vec<Option<Var>>,
    types: Vec<Option<Vec<Var>>>,
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
            Some(f) => flux_solve::traits::projections_of(items, sigs, f),
            None => Vec::new(),
        };
        Cx {
            body,
            items,
            sigs,
            env,
            header_aliases: Vec::new(),
            family_depth: 0,
            family_goals: Vec::new(),
            caller_goals_in_progress: None,
            declaration_stack: Vec::new(),
            header_predicates: body.owner.map_or(Vec::new(), |f| {
                flux_solve::traits::elaborate(
                    sigs,
                    &flux_solve::traits::predicates_of(items, sigs, f),
                )
            }),
            unsized_params: body.owner.map_or(Vec::new(), |f| {
                let function = &items.functions[f];
                flux_solve::unsized_params(items, function.owner, &function.generics)
            }),
            slot_counts: FxHashMap::default(),
            selected_source_signatures: FxHashMap::default(),
            local_base: Vec::new(),
            vars: 0,
            universal: Vec::new(),
            universal_of: FxHashMap::default(),
            known_pairs: Vec::new(),
            known: Vec::new(),
            known_types: Vec::new(),
            in_params: Vec::new(),
            env_regions: Vec::new(),
            proving: Category::CallArgument,
            constraints: Vec::new(),
            type_tests: Vec::new(),
            loan_vars: Vec::new(),
            by_sup: Adjacency::default(),
            closure_ext: Vec::new(),
            requirements,
            own_requirements: Vec::new(),
            own_type_requirements: Vec::new(),
            errors: Vec::new(),
            hidden: Vec::new(),
            holes: Default::default(),
            hole_pairs: Default::default(),
            pointer_checks: Vec::new(),
            call_inputs: Vec::new(),
            proven_closures: Default::default(),
            output_closures: Default::default(),
            call_range: TextRange::default(),
            env_late: Vec::new(),
            env_ranked: body.owner.map_or_else(Vec::new, |f| {
                ranked_projections::for_function(items, sigs, f)
            }),
            late_inputs: Vec::new(),
            call_projections: Vec::new(),
        }
    }

    /// Concrete associated-type normalization may itself select an impl
    /// with region-dependent bounds. Until that selection can be proved here,
    /// only projections guaranteed by the header's own bounds are available.
    fn impl_aliases_known(&self, ty: Ty, args: &[Ty]) -> bool {
        let ty = if args.is_empty() { ty } else { ty.subst(args) };
        if let TyKind::Alias(alias) = ty.kind()
            && !self.env.iter().any(|p| p.alias == alias)
            && !self.header_aliases.contains(&alias)
            && !(self.items.traits[alias.trait_]
                .generics
                .lifetimes
                .is_empty()
                && alias
                    .args
                    .iter()
                    .all(|&arg| flux_ty::regions::slots(self.items, arg) == 0)
                && self.header_predicates.contains(&flux_ty::Predicate {
                    self_ty: alias.args[0],
                    trait_ref: flux_ty::TraitRef {
                        trait_: alias.trait_,
                        name: alias.trait_name,
                        args: &alias.args[1..],
                    },
                }))
        {
            return false;
        }
        ty.children().all(|t| self.impl_aliases_known(t, &[]))
    }

    /// Prove nested, region-free selections before using their normalized RHS.
    /// Region-bearing projections still need their occurrence tables.
    fn impl_aliases_proven(
        &mut self,
        ty: Ty,
        args: &[Ty],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> bool {
        if depth > DECLARATION_RECURSION_LIMIT {
            return false;
        }
        let ty = if args.is_empty() { ty } else { ty.subst(args) };
        if self.impl_aliases_known(ty, &[]) {
            return true;
        }
        for child in ty.children() {
            if !self.impl_aliases_proven(child, &[], env, depth + 1) {
                return false;
            }
        }
        let TyKind::Alias(alias) = ty.kind() else {
            return true;
        };
        if flux_ty::regions::has_projection_inputs(self.items, alias)
            || !self.items.traits[alias.trait_]
                .generics
                .lifetimes
                .is_empty()
            || alias.args.iter().any(|&arg| self.slots(arg) != 0)
            || self.slots(self.normalize(ty)) != 0
        {
            return false;
        }
        // Child selections were proved above. Match the outer impl against
        // their normalized types, retaining the original proof obligations.
        let predicate = flux_ty::Predicate {
            self_ty: self.normalize(alias.args[0]),
            trait_ref: flux_ty::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: Ty::list(
                    &alias.args[1..]
                        .iter()
                        .map(|&ty| self.normalize(ty))
                        .collect::<Vec<_>>(),
                ),
            },
        };
        let goal = ImplRegionGoal {
            lifetimes: Vec::new(),
            predicate,
            vars: vec![Vec::new(); alias.args.len()],
            bound: Vec::new(),
        };
        if !self.impl_prove_super(&goal, env, 0) {
            return false;
        }
        if self.normalize(ty) == ty
            && env.iter().any(|known| {
                known.predicate == predicate
                    && known.lifetimes.is_empty()
                    && known.vars.iter().all(Vec::is_empty)
            })
        {
            // An original membership owns this abstract output. No selected
            // concrete RHS exists whose prerequisites need to be invented.
            return true;
        }
        if self.declared_region_free_bound(&goal, env) {
            // A header's abstract output supplies membership and its declared
            // equalities; there is no concrete impl RHS to select here.
            return true;
        }
        let Some((imp, impl_args)) =
            flux_solve::traits::select_impl(self.items, self.sigs, predicate)
        else {
            return false;
        };
        let Some(value) = flux_solve::traits::impl_type(self.sigs, imp, alias.name) else {
            return false;
        };
        self.impl_aliases_proven(value, &impl_args, env, depth + 1)
    }

    /// An unchanged abstract projection stores its captured input row. Keep
    /// that representation through a substituted container, while refusing to
    /// reinterpret a selected concrete output's fields as projection inputs.
    fn represented_value_aliases_proven(
        &mut self,
        ty: Ty,
        row: &[Var],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> bool {
        if depth > DECLARATION_RECURSION_LIMIT
            || row.len() != self.slots(ty) as usize
            || self.normalize(ty) != ty
        {
            return false;
        }
        if !ty.has_aliases() {
            return true;
        }
        if let TyKind::Alias(alias) = ty.kind() {
            return self
                .proven_family_output(alias, row, env, depth + 1)
                .is_some();
        }
        let (prefix, children): (usize, Vec<Ty>) = match ty.kind() {
            TyKind::Tuple(types) => (0, types.to_vec()),
            TyKind::Adt(adt, args) => (
                self.items.structs[adt.strukt].generics.lifetimes.len(),
                args.to_vec(),
            ),
            TyKind::Ref(_, inner) => (1, vec![inner]),
            TyKind::RawPtr(_, inner) | TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                (0, vec![inner])
            }
            _ => return false,
        };
        let mut offset = prefix;
        for child in children {
            let end = offset + self.slots(child) as usize;
            let Some(child_row) = row.get(offset..end) else {
                return false;
            };
            if !self.represented_value_aliases_proven(child, child_row, env, depth + 1) {
                return false;
            }
            offset = end;
        }
        offset == row.len()
    }

    /// Prove legacy projection selections using only represented generic
    /// argument rows. No owner/family lifetime table is invented for an alias.
    fn instantiated_aliases_proven(
        &mut self,
        ty: Ty,
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> bool {
        if depth > DECLARATION_RECURSION_LIMIT {
            return false;
        }
        if self.impl_aliases_known(ty, args) {
            return true;
        }
        // A substituted parameter supplies only its value's output row. It
        // cannot also supply the original inputs of a hidden projection.
        if let TyKind::Param(parameter) = ty.kind() {
            if self.impl_aliases_proven(ty, args, env, depth) {
                return true;
            }
            let instance = ty.subst(args);
            // An original bound on this exact value occurrence already
            // establishes its formation. Reuse that value's output row, not
            // an invented set of inputs for its hidden projection.
            if let Some(row) = arg_vars.get(parameter.index as usize)
                && env.iter().any(|known| {
                    known.predicate.self_ty == self.normalize(instance)
                        && known.vars.first() == Some(row)
                })
            {
                return true;
            }
            if let Some(row) = arg_vars.get(parameter.index as usize)
                && self.represented_value_aliases_proven(instance, row, env, depth + 1)
            {
                return true;
            }
            // A logical abstract output can itself be passed as a type
            // argument. Its selection inputs must still be reconstructible;
            // its supplied output row is not reused as an input row.
            return instance != ty
                && self.instantiated_aliases_proven(instance, &[], &[], env, depth + 1);
        }
        for child in ty.children() {
            if !self.instantiated_aliases_proven(child, args, arg_vars, env, depth + 1) {
                return false;
            }
        }
        let TyKind::Alias(template) = ty.kind() else {
            return true;
        };
        if flux_ty::regions::has_projection_inputs(self.items, template)
            || !self.items.traits[template.trait_]
                .generics
                .lifetimes
                .is_empty()
        {
            return false;
        }
        let mut inputs = Vec::new();
        let mut rows = Vec::new();
        for &input in template.args {
            // A Param obtains its complete existing row from arg_vars. A
            // written reference/ADT lifetime without a selection table fails.
            let Some(row) = self.instantiate_sig(input, &[], &mut |_| None, args, arg_vars) else {
                return false;
            };
            let input = self.normalize(if args.is_empty() {
                input
            } else {
                input.subst(args)
            });
            if row.len() != self.slots(input) as usize {
                return false;
            }
            inputs.push(input);
            rows.push(row);
        }
        let predicate = flux_ty::Predicate {
            self_ty: inputs[0],
            trait_ref: flux_ty::TraitRef {
                trait_: template.trait_,
                name: template.trait_name,
                args: Ty::list(&inputs[1..]),
            },
        };
        let goal = ImplRegionGoal {
            predicate,
            lifetimes: Vec::new(),
            vars: rows.clone(),
            bound: Vec::new(),
        };
        if !self.impl_prove_super(&goal, env, depth + 1) {
            return false;
        }
        let projection = Ty::alias(AliasTy {
            args: Ty::list(&inputs),
            ..template
        });
        if let TyKind::Dynamic(object) = predicate.self_ty.kind()
            && let Some(declared) = flux_solve::traits::dyn_projections(
                self.items,
                self.sigs,
                predicate.self_ty,
                object,
            )
            .into_iter()
            .find(|declared| Ty::alias(declared.alias) == projection)
        {
            // The full principal/argument rows were proved above. An object's
            // declared equality selects no concrete impl; any further RHS
            // selection must still be independently represented and proved.
            return self.instantiated_aliases_proven(declared.ty, &[], &[], env, depth + 1);
        }
        // Membership from an original caller assumption justifies a
        // still-abstract output. It does not choose a candidate impl's RHS
        // or supply any outlives bound on that output.
        if self.normalize(projection) == projection
            && env.iter().any(|known| {
                known.predicate == goal.predicate
                    && known.lifetimes.len() == goal.lifetimes.len()
                    && known.vars.len() == goal.vars.len()
                    && known
                        .vars
                        .iter()
                        .zip(&goal.vars)
                        .all(|(a, b)| a.len() == b.len())
            })
        {
            return true;
        }
        if let TyKind::Alias(owner) = predicate.self_ty.kind()
            && owner.family_lifetimes == 0
            && owner.family_args.is_empty()
            && let Some(equality) = flux_solve::traits::alias_projections(self.sigs, owner)
                .into_iter()
                .find(|equality| Ty::alias(equality.alias) == projection)
            && let TyKind::Alias(sibling) = equality.ty.kind()
            && sibling.trait_ == owner.trait_
            && sibling.args == owner.args
            && sibling.owner_lifetimes == owner.owner_lifetimes
            && sibling.family_lifetimes == 0
            && sibling.family_args.is_empty()
            && self.normalize(equality.ty) == equality.ty
            && let Some(row) = rows.first()
            && self
                .proven_family_output(owner, row, env, depth + 1)
                .is_some()
            && self
                .proven_family_output(sibling, row, env, depth + 1)
                .is_some()
        {
            // A declared equality forwards to another abstract output of the
            // same represented owner. Both selections use the original inputs;
            // neither needs a concrete impl for that caller-owned value.
            return true;
        }
        let Some((imp, impl_args)) = self.matching_impl(predicate) else {
            return false;
        };
        // An assumption proves membership, not the extra conditions of a
        // concrete impl chosen for normalization. Prove that selected path
        // without using the required trait itself as its own assumption.
        if env.iter().any(|known| known.predicate == goal.predicate) {
            let selected_env: Vec<_> = env
                .iter()
                .filter(|known| known.predicate != goal.predicate)
                .cloned()
                .collect();
            if !self.impl_prove_super(&goal, &selected_env, depth + 1) {
                return false;
            }
        }
        let Some(value) = flux_solve::traits::impl_type(self.sigs, imp, template.name) else {
            return false;
        };
        if self.impl_aliases_known(value, &impl_args)
            || self.impl_aliases_proven(value, &impl_args, env, depth + 1)
        {
            return true;
        }
        let Some(binding) = self.bind_impl(
            imp,
            &impl_args,
            &inputs,
            &rows,
            &[],
            TextRange::default(),
            depth + 1,
            None,
        ) else {
            return false;
        };
        // An unbound impl parameter cannot acquire an output row from the
        // type system's erased selection. Every row must come from its header.
        let Some(type_vars) = binding.types.into_iter().collect::<Option<Vec<_>>>() else {
            return false;
        };
        self.instantiated_aliases_proven(value, &impl_args, &type_vars, env, depth + 1)
    }

    /// Instantiate an impl bound without inventing lifetimes for missing
    /// metadata. An unrepresentable obligation must fail this universal check.
    #[allow(clippy::too_many_arguments)]
    fn impl_region_goal(
        &mut self,
        p: flux_ty::Predicate,
        table: &[Region],
        trait_regions: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
    ) -> Option<ImplRegionGoal> {
        self.impl_region_goal_with_proof(
            p,
            table,
            trait_regions,
            lifetimes,
            args,
            arg_vars,
            None,
            false,
        )
    }

    /// Selected impl predicates may normalize nested, region-free projections
    /// only after their selection requirements have been discharged.
    #[allow(clippy::too_many_arguments)]
    fn impl_region_goal_with_proof(
        &mut self,
        p: flux_ty::Predicate,
        table: &[Region],
        trait_regions: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        proof: Option<(&[ImplRegionGoal], u32)>,
        known_self: bool,
    ) -> Option<ImplRegionGoal> {
        self.impl_region_goal_with_inputs(
            p,
            table,
            trait_regions,
            lifetimes,
            args,
            arg_vars,
            proof,
            known_self,
            proof.map(|(environment, _)| environment),
        )
    }

    /// Original caller bounds and complete argument rows can prove nested
    /// selections. Written inputs without occurrence tables stay unsupported.
    #[allow(clippy::too_many_arguments)]
    fn impl_region_goal_with_inputs(
        &mut self,
        p: flux_ty::Predicate,
        table: &[Region],
        trait_regions: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        proof: Option<(&[ImplRegionGoal], u32)>,
        known_self: bool,
        projection_inputs: Option<&[ImplRegionGoal]>,
    ) -> Option<ImplRegionGoal> {
        if trait_regions.lifetimes.len()
            != self.items.traits[p.trait_ref.trait_]
                .generics
                .lifetimes
                .len()
        {
            return None;
        }
        let callable = flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_some();
        let mut named_bound = FxHashMap::default();
        let mut bound = Vec::new();
        for index in 0..trait_regions.bound {
            let var = self.new_universal(format!("'bound{index}"));
            named_bound.insert(index, var);
            bound.push(var);
        }
        for &region in table
            .iter()
            .chain(trait_regions.lifetimes.iter())
            .chain(trait_regions.args.iter().flat_map(|row| row.iter()))
        {
            if let Region::Late(index) = region {
                if trait_regions.bound != 0 && index >= trait_regions.bound {
                    return None;
                }
                if trait_regions.bound == 0 && !callable {
                    return None;
                }
                let var = *named_bound
                    .entry(index)
                    .or_insert_with(|| self.new_universal(format!("'bound{index}")));
                if !bound.contains(&var) {
                    bound.push(var);
                }
            }
        }
        let trait_lifetimes = trait_regions
            .lifetimes
            .iter()
            .map(|&region| match region {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied(),
                Region::Late(i) => named_bound.get(&i).copied(),
                Region::Erased => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let mut vars = Vec::new();
        let mut types = Vec::new();
        for (k, &ty) in std::iter::once(&p.self_ty)
            .chain(p.trait_ref.args)
            .enumerate()
        {
            let table = if k == 0 {
                table
            } else {
                trait_regions.args.get(k - 1).map_or(&[][..], |t| t)
            };
            let mut aliases_proven = (k == 0 && known_self)
                || self.impl_aliases_known(ty, args)
                || proof.is_some_and(|(env, depth)| self.impl_aliases_proven(ty, args, env, depth))
                || projection_inputs.is_some_and(|env| {
                    self.instantiated_aliases_proven(ty, args, arg_vars, env, 0)
                });
            if !aliases_proven && projection_inputs.is_none() {
                return None;
            }
            // Erased regions in the tuple of a parenthesized Fn bound are
            // that bound's fresh input lifetimes, not missing header lifetimes.
            // Generic arguments contribute their existing slots unchanged.
            let fresh: Vec<_> = if callable && k == 1 {
                table
                    .iter()
                    .filter(|&&r| r == Region::Erased)
                    .map(|_| self.new_universal("'callback".into()))
                    .collect()
            } else {
                Vec::new()
            };
            bound.extend(fresh.iter().copied());
            let mut fresh = fresh.into_iter();
            let mut map = |r| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => lifetimes.get(i as usize).copied(),
                Region::Late(i) => named_bound.get(&i).copied(),
                Region::Erased => fresh.next(),
            };
            if !aliases_proven
                && let TyKind::Alias(alias) = ty.kind()
                && let Some(env) = projection_inputs
            {
                aliases_proven =
                    self.written_family_alias_proven(alias, table, &mut map, args, arg_vars, env);
            }
            let slots = self.instantiate_sig(ty, table, &mut map, args, arg_vars)?;
            // Proving a selected occurrence may resolve this associated type.
            let original = if args.is_empty() { ty } else { ty.subst(args) };
            let instance = self.normalize(original);
            if slots.len() != self.slots(instance) as usize {
                return None;
            }
            // Written projection arguments carry occurrence rows too. Prove
            // their owners after instantiating that row, rather than inventing
            // inputs from their erased type or from generic arguments alone.
            if !aliases_proven
                && !projection_inputs.is_some_and(|env| {
                    self.represented_value_aliases_proven(original, &slots, env, 0)
                })
            {
                return None;
            }
            types.push(instance);
            vars.push(slots);
        }
        Some(ImplRegionGoal {
            lifetimes: trait_lifetimes,
            predicate: flux_ty::Predicate {
                self_ty: types[0],
                trait_ref: flux_ty::TraitRef {
                    args: Ty::list(&types[1..]),
                    ..p.trait_ref
                },
            },
            vars,
            bound,
        })
    }

    /// Trait parameters carry their declaration's prerequisites at every
    /// occurrence. A root membership is not evidence for those requirements.
    fn trait_parameters_hold(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<bool> {
        let id = goal.predicate.trait_ref.trait_;
        let predicates = self.sigs.trait_predicates.get(id).map_or(&[][..], |p| p);
        if predicates.is_empty() {
            return Some(true);
        }
        let args: Vec<_> = std::iter::once(goal.predicate.self_ty)
            .chain(goal.predicate.trait_ref.args.iter().copied())
            .collect();
        let required = self.lifetime_trait_parameters(goal.predicate, depth);
        if required.is_empty() {
            return Some(true);
        }
        let (rows, bounds, _) = flux_ty::regions::trait_bound_regions(self.items, self.sigs, id)?;
        if rows.len() != predicates.len() || bounds.len() != predicates.len() {
            return None;
        }
        for index in required {
            let required = self.impl_region_goal_with_proof(
                predicates[index],
                &rows[index],
                &bounds[index],
                &goal.lifetimes,
                &args,
                &goal.vars,
                Some((env, depth)),
                false,
            )?;
            if !self.impl_prove_super(&required, env, depth + 1) {
                return Some(false);
            }
        }
        Some(true)
    }

    /// Only declaration prerequisites that select lifetime conditions need
    /// this region proof. Ordinary type-only bounds use the type solver.
    fn lifetime_trait_parameters(&self, goal: flux_ty::Predicate, depth: u32) -> Vec<usize> {
        let args: Vec<_> = std::iter::once(goal.self_ty)
            .chain(goal.trait_ref.args.iter().copied())
            .collect();
        self.sigs
            .trait_predicates
            .get(goal.trait_ref.trait_)
            .into_iter()
            .flatten()
            .enumerate()
            .filter_map(|(index, &predicate)| {
                self.requires_lifetime_proof(
                    predicate.subst(&args),
                    &mut FxHashSet::default(),
                    depth,
                )
                .then_some(index)
            })
            .collect()
    }

    /// Elaborate while retaining the slots contributed by Self and generic
    /// arguments. Written reference lifetimes in supertrait arguments do not
    /// yet have tables; reject that boundary instead of guessing their regions.
    fn impl_super_goals(
        &mut self,
        goal: ImplRegionGoal,
        out: &mut Vec<ImplRegionGoal>,
        depth: u32,
    ) -> bool {
        self.impl_super_goals_with_inputs(goal, out, depth, None)
    }

    /// Required declaration bounds may select projections using the original
    /// header environment. Generated supergoals never become assumptions.
    fn impl_super_goals_with_inputs(
        &mut self,
        goal: ImplRegionGoal,
        out: &mut Vec<ImplRegionGoal>,
        depth: u32,
        projection_inputs: Option<&[ImplRegionGoal]>,
    ) -> bool {
        if depth > DECLARATION_RECURSION_LIMIT {
            return false;
        }
        if out.iter().any(|g| {
            g.predicate == goal.predicate
                && g.lifetimes == goal.lifetimes
                && g.vars == goal.vars
                && g.bound == goal.bound
        }) {
            return true;
        }
        out.push(goal.clone());
        let args: Vec<_> = std::iter::once(goal.predicate.self_ty)
            .chain(goal.predicate.trait_ref.args.iter().copied())
            .collect();
        let trait_id = goal.predicate.trait_ref.trait_;
        let supers = self
            .sigs
            .traits
            .get(trait_id)
            .map_or(&[][..], |s| &s.supertraits);
        let tables = self
            .sigs
            .regions
            .traits
            .get(trait_id)
            .map_or(&[][..], |r| &r.supertraits);
        if supers.len() != tables.len() {
            return false;
        }
        for (&sup, table) in supers.iter().zip(tables) {
            // Self is the same represented input on every supertrait edge.
            // Reuse its row rather than reselecting an abstract projection.
            let Some(mut next) = self.impl_region_goal_with_inputs(
                sup.with_self(Ty::param(0, flux_intern::kw::SelfUpper)),
                &[],
                table,
                &goal.lifetimes,
                &args,
                &goal.vars,
                None,
                true,
                projection_inputs,
            ) else {
                return false;
            };
            for &var in &goal.bound {
                if !next.bound.contains(&var) {
                    next.bound.push(var);
                }
            }
            if !self.impl_super_goals_with_inputs(next, out, depth + 1, projection_inputs) {
                return false;
            }
        }
        true
    }

    /// Prove the equalities on each elaborated trait edge using only the
    /// original impl header environment. In particular, never normalize with
    /// the required equality installed as an assumption of its own proof.
    fn impl_super_equalities(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
    ) -> Option<bool> {
        self.impl_equalities(goal, env, false)
    }

    fn impl_equalities(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        associated: bool,
    ) -> Option<bool> {
        let args: Vec<_> = std::iter::once(goal.predicate.self_ty)
            .chain(goal.predicate.trait_ref.args.iter().copied())
            .collect();
        let declaration = self.sigs.traits.get(goal.predicate.trait_ref.trait_)?;
        let header_rows = if !associated && !declaration.header_projections.is_empty() {
            flux_ty::regions::trait_bound_regions(
                self.items,
                self.sigs,
                goal.predicate.trait_ref.trait_,
            )?
            .2
        } else {
            Box::default()
        };
        let items = self.items;
        for p in self
            .sigs
            .traits
            .get(goal.predicate.trait_ref.trait_)
            .map_or(&[][..], |s| &s.implied)
            .iter()
            // Keep associated-value equalities separate from Self's
            // supertrait edges, including when the trait has both.
            .filter(|p| {
                let self_param = Ty::param(0, flux_intern::kw::SelfUpper);
                if associated {
                    matches!(p.alias.args.first().map(|t| t.kind()), Some(TyKind::Alias(a)) if a.trait_ == goal.predicate.trait_ref.trait_ && a.args.first() == Some(&self_param) && !flux_ty::regions::has_projection_inputs(items, a))
                } else { p.alias.args.first() == Some(&self_param) }
            })
        {
            // Associated output contracts retain written lifetime rows,
            // including static references nested in equality RHS tuples.
            if !self.impl_aliases_known(p.ty, &args)
                && !self.impl_aliases_proven(p.ty, &args, env, 0)
            {
                return None;
            }
            let expected_ty = self.normalize(p.ty.subst(&args));
            let table = if associated {
                self.sigs.regions.traits.get(goal.predicate.trait_ref.trait_)
                    .into_iter().flat_map(|r| r.families.iter())
                    .flat_map(|(_, family)| family.output_projections.iter())
                    .find(|(projection, _)| projection == p)
                    .map_or(&[][..], |(_, row)| row.as_ref())
            } else {
                declaration.header_projections.iter().position(|projection| projection == p)
                    .and_then(|index| header_rows.get(index)).map_or(&[][..], |row| row.as_ref())
            };
            let expected = self.instantiate_sig(p.ty, table, &mut |region| match region {
                Region::Static => Some(STATIC),
                Region::Param(i) => goal.lifetimes.get(i as usize).copied(),
                Region::Erased | Region::Late(_) => None,
            }, &args, &goal.vars)?;
            if expected.len() != self.slots(expected_ty) as usize {
                return None;
            }
            let alias_goal = self.impl_region_goal(
                flux_ty::Predicate {
                    self_ty: p.alias.args[0],
                    trait_ref: flux_ty::TraitRef {
                        trait_: p.alias.trait_,
                        name: p.alias.trait_name,
                        args: &p.alias.args[1..],
                    },
                },
                &[],
                &TraitRefRegions::default(),
                &[],
                &args,
                &goal.vars,
            )?;
            if !self.impl_prove_super(&alias_goal, env, 0) {
                return None;
            }
            let alias = if associated {
                let alias_args: Vec<_> = std::iter::once(alias_goal.predicate.self_ty)
                    .chain(alias_goal.predicate.trait_ref.args.iter().copied())
                    .collect();
                AliasTy { args: Ty::list(&alias_args), ..p.alias }
            } else {
                let TyKind::Alias(alias) = Ty::alias(p.alias).subst(&args).kind() else {
                    return None;
                };
                alias
            };
            // A still-abstract output, or an equality inherited from a
            // proven type-only declaration bound, selects no concrete RHS.
            if !self.env.iter().any(|p| p.alias == alias)
                && self.normalize(Ty::alias(alias)) != Ty::alias(alias)
                && !self.declared_region_free_bound(&alias_goal, env)
            {
                let (imp, impl_args) =
                    self.matching_impl(alias_goal.predicate).or_else(||flux_solve::traits::select_impl(self.items, self.sigs, alias_goal.predicate)
                        .filter(|(_,args)|args.iter().all(|ty|!ty.has_error())))?;
                let value = flux_solve::traits::impl_type(self.sigs, imp, alias.name)?;
                // Proving the outer edge does not prove the selected RHS's
                // nested projection requirements. Discharge those separately.
                if !self.impl_aliases_proven(value, &impl_args, env, 0) {
                    let header: Vec<_> = std::iter::once(alias_goal.predicate.self_ty)
                        .chain(alias_goal.predicate.trait_ref.args.iter().copied())
                        .collect();
                    let binding = self.bind_impl(
                        imp, &impl_args, &header, &alias_goal.vars,
                        &alias_goal.lifetimes, TextRange::default(), 0, Some(env),
                    )?;
                    let rows = binding.types.into_iter().zip(&impl_args).map(|(row, &ty)| {
                        match row {
                            Some(row) => Some(row),
                            None if self.slots(ty) == 0 => Some(Vec::new()),
                            None => None,
                        }
                    }).collect::<Option<Vec<_>>>()?;
                    if !self.instantiated_aliases_proven(value, &impl_args, &rows, env, 0) {
                        return None;
                    }
                }
            }
            let actual_ty = self.normalize(Ty::alias(alias));
            if actual_ty != expected_ty {
                return Some(false);
            }
            let actual = if !p.alias.family_args.is_empty() {
                // Associated-output edges need their own input occurrence
                // tables; do not borrow an erased-equal Self's regions.
                if associated{return None;}
                self.instantiate_sig(Ty::alias(p.alias),&[],&mut |_|None,&args,&goal.vars)?
            } else if self.slots(actual_ty) == 0 {
                Vec::new()
            } else {
                self.normalize_regions(alias, &alias_goal.vars, 0)?
            };
            if actual.len() != expected.len() {
                return None;
            }
            for (a, b) in actual.into_iter().zip(expected) {
                self.equate(a, b, Category::Boring, TextRange::default());
            }
        }
        Some(true)
    }

    /// Select a candidate header; callers still prove every required bound.
    fn matching_impl(&self, goal: flux_ty::Predicate) -> Option<(flux_hir::ImplId, Vec<Ty>)> {
        flux_solve::traits::RigidSolver::new(
            self.items,
            self.sigs,
            &self.header_predicates,
            &self.env,
            &self.unsized_params,
        )
        .candidate(goal)
        .filter(|(_, args)| args.iter().all(|ty| !ty.has_error()))
    }

    /// Type-only declaration bounds follow from the original header predicates.
    fn declared_region_free_bound(&self, goal: &ImplRegionGoal, env: &[ImplRegionGoal]) -> bool {
        let TyKind::Alias(alias) = goal.predicate.self_ty.kind() else {
            return false;
        };
        if flux_ty::regions::has_projection_inputs(self.items, alias)
            || !self.items.traits[alias.trait_]
                .generics
                .lifetimes
                .is_empty()
            || !goal.lifetimes.is_empty()
            || !goal.bound.is_empty()
            || goal.vars.iter().any(|row| !row.is_empty())
            || alias
                .args
                .iter()
                .any(|&ty| flux_ty::regions::slots(self.items, ty) != 0)
        {
            return false;
        }
        let header: Vec<_> = env.iter().map(|goal| goal.predicate).collect();
        flux_solve::traits::alias_bounds(self.sigs, &header, goal.predicate.self_ty)
            .contains(&goal.predicate)
    }

    fn impl_prove_super(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> bool {
        let coinductive = self.items.traits[goal.predicate.trait_ref.trait_]
            .lang
            .is_some_and(|lang| matches!(lang.as_str(), "send" | "sync"));
        if coinductive
            && let Some(start) = self
                .declaration_stack
                .iter()
                .position(|(known, _)| known == goal)
            && self.declaration_stack[start..]
                .iter()
                .all(|(_, auto)| *auto)
        {
            return true;
        }
        self.declaration_stack.push((goal.clone(), coinductive));
        let result = self.impl_prove_super_step(goal, env, depth);
        self.declaration_stack.pop();
        result
    }

    fn impl_prove_super_step(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> bool {
        if depth > DECLARATION_RECURSION_LIMIT {
            return false;
        }
        let range = TextRange::default();
        if self.items.traits[goal.predicate.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
            || flux_solve::traits::is_builtin_trait(
                self.items,
                goal.predicate.trait_ref.trait_,
                goal.predicate.self_ty,
            )
        {
            return flux_solve::traits::holds(self.items, self.sigs, &[], goal.predicate);
        }
        self.close_known();
        let mut candidates = Vec::new();
        for known in env.iter().filter(|g| g.predicate == goal.predicate) {
            if known.lifetimes.len() != goal.lifetimes.len()
                || known.vars.len() != goal.vars.len()
                || known
                    .vars
                    .iter()
                    .zip(&goal.vars)
                    .any(|(a, b)| a.len() != b.len())
            {
                continue;
            }
            let mut instantiated = FxHashMap::default();
            let mut equalities = Vec::new();
            for (a, b) in known
                .lifetimes
                .iter()
                .copied()
                .zip(goal.lifetimes.iter().copied())
                .chain(
                    known
                        .vars
                        .iter()
                        .zip(&goal.vars)
                        .flat_map(|(a, b)| a.iter().copied().zip(b.iter().copied())),
                )
            {
                if known.bound.contains(&a) {
                    if let Some(&previous) = instantiated.get(&a) {
                        equalities.push((previous, b));
                    } else {
                        instantiated.insert(a, b);
                    }
                } else {
                    equalities.push((a, b));
                }
            }
            // Prefer an assumption whose rigid universals already agree. Do
            // not let a static-only bound hide a polymorphic bound in scope.
            let conflicts = equalities
                .iter()
                .filter(
                    |(a, b)| match (self.universal_of.get(a), self.universal_of.get(b)) {
                        (Some(&a), Some(&b)) => !self.known(a, b) || !self.known(b, a),
                        _ => false,
                    },
                )
                .count();
            candidates.push((conflicts, equalities));
        }
        if let Some((_, equalities)) = candidates
            .into_iter()
            .min_by_key(|(conflicts, _)| *conflicts)
        {
            for (a, b) in equalities {
                self.equate(a, b, Category::Boring, range);
            }
            return true;
        }
        if self.declared_region_free_bound(goal, env) {
            return true;
        }
        if let Some(bounds) = self.associated_output_bounds(goal, env, depth)
            && bounds.iter().any(|bound| bound.predicate == goal.predicate)
            && self.impl_prove_super(goal, &bounds, depth + 1)
        {
            return true;
        }
        if let Some(proven) = self.prove_auto_super(goal, env, depth) {
            return proven;
        }
        if let Some(bounds) = self.object_super_goals(goal)
            && bounds.iter().any(|bound| bound.predicate == goal.predicate)
        {
            // Membership requires the whole represented Self/argument type to
            // satisfy the declaration, beyond merely forming an object type.
            if self.object_membership_outlives(goal).is_none() {
                return false;
            }
            return self.impl_prove_super(goal, &bounds, depth + 1);
        }
        if let TyKind::Dynamic(object) = goal.predicate.self_ty.kind()
            && let (Some(source), Some(required)) = (
                flux_ty::fn_trait_kind(self.items, object.trait_),
                flux_ty::fn_trait_kind(self.items, goal.predicate.trait_ref.trait_),
            )
        {
            if source <= required
                && object.args == goal.predicate.trait_ref.args
                && let Some(signature) = object.callable_signature()
            {
                let TyKind::Tuple(types) = object.args.first().copied().unwrap_or(Ty::ERROR).kind()
                else {
                    return false;
                };
                let (Some(own), Some(input)) = (goal.vars.first(), goal.vars.get(1)) else {
                    return false;
                };
                let Some(own) = own.get(1..) else {
                    return false;
                };
                let mut offset = 0;
                let mut rows = Vec::new();
                for &ty in types {
                    let count = self.slots(ty) as usize;
                    let Some(row) = input.get(offset..offset + count) else {
                        return false;
                    };
                    rows.push(row.to_vec());
                    offset += count;
                }
                return offset == input.len()
                    && self
                        .pointer_inputs(signature, own, types, &rows, range)
                        .is_some();
            }
            // A callable object's principal supplies its callable membership.
            // Input lifetimes need a separate binder recipe; zero-slot inputs
            // have no relationship to the object's own lifetime bound.
            return source <= required
                && object.args == goal.predicate.trait_ref.args
                && goal.lifetimes.is_empty()
                && goal.vars.len() == 2
                && goal.vars[0].len() == self.slots(goal.predicate.self_ty) as usize
                && goal.vars[1].is_empty()
                && object.args.iter().all(|&ty| self.slots(ty) == 0);
        }
        if flux_ty::fn_trait_kind(self.items, goal.predicate.trait_ref.trait_).is_some()
            && let TyKind::Closure(id, list) = goal.predicate.self_ty.kind()
        {
            if !goal.lifetimes.is_empty()
                || goal.vars.len() != 2
                || goal.vars[0].len() != self.slots(goal.predicate.self_ty) as usize
                || goal.predicate.trait_ref.args.len() != 1
                || goal.vars[1].len() != self.slots(goal.predicate.trait_ref.args[0]) as usize
                || !flux_solve::traits::holds(self.items, self.sigs, &[], goal.predicate)
            {
                return false;
            }
            // Selected callable adapters still invoke the original closure.
            // Reuse its exported requirements with these exact capture/input
            // rows; result relations are proved by the Output equality.
            self.closure_goal(
                id,
                list,
                Some(&goal.vars[0]),
                Some(&goal.vars[1]),
                None,
                range,
            );
            return true;
        }
        if flux_ty::fn_trait_kind(self.items, goal.predicate.trait_ref.trait_).is_some()
            && let TyKind::FnPtr(signature) = goal.predicate.self_ty.kind()
        {
            if !flux_solve::traits::holds(self.items, self.sigs, &[], goal.predicate) {
                return false;
            }
            let Some(&tuple) = goal.predicate.trait_ref.args.first() else {
                return false;
            };
            let TyKind::Tuple(types) = tuple.kind() else {
                return false;
            };
            let (Some(own), Some(input)) = (goal.vars.first(), goal.vars.get(1)) else {
                return false;
            };
            let mut offset = 0;
            let mut rows = Vec::new();
            for &ty in types {
                let count = self.slots(ty) as usize;
                let Some(row) = input.get(offset..offset + count) else {
                    return false;
                };
                rows.push(row.to_vec());
                offset += count;
            }
            return offset == input.len()
                && self
                    .pointer_inputs(signature, own, types, &rows, range)
                    .is_some();
        }
        let selected = self.matching_impl(goal.predicate);
        let Some((imp, args)) = selected else {
            // Built-in, region-free obligations (notably Sized and Copy)
            // need no user impl. Never use erased environment bounds here.
            return goal.vars.iter().all(Vec::is_empty)
                && flux_solve::traits::holds(self.items, self.sigs, &[], goal.predicate);
        };
        let Some(block) = self.sigs.regions.impls.get(imp) else {
            return false;
        };
        let header: Vec<_> = std::iter::once(goal.predicate.self_ty)
            .chain(goal.predicate.trait_ref.args.iter().copied())
            .collect();
        let Some(binding) = self.bind_impl(
            imp,
            &args,
            &header,
            &goal.vars,
            &goal.lifetimes,
            range,
            depth,
            Some(env),
        ) else {
            return false;
        };
        let lifetimes: Vec<_> = binding
            .lifetimes
            .into_iter()
            .map(|v| v.unwrap_or_else(|| self.fresh()))
            .collect();
        let type_vars: Vec<_> = binding
            .types
            .into_iter()
            .enumerate()
            .map(|(i, v)| {
                v.unwrap_or_else(|| {
                    let n = self.slots(args[i]);
                    self.fresh_list(n)
                })
            })
            .collect();
        let map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        for p in block.bounds.iter() {
            match *p {
                Outlives::Region(a, b) => {
                    let (Some(a), Some(b)) = (map(a), map(b)) else {
                        return false;
                    };
                    self.outlives(a, b, Category::Boring, range);
                }
                Outlives::Type(t, b) => {
                    let Some(b) = map(b) else { return false };
                    let Some(vars) = self.instantiate_sig(t, &[], &mut { map }, &args, &type_vars)
                    else {
                        return false;
                    };
                    for v in vars {
                        self.outlives(v, b, Category::Boring, range);
                    }
                    self.type_outlives(self.normalize(t.subst(&args)), b, range);
                }
            }
        }
        let mut proven = Vec::new();
        for (k, &p) in self
            .sigs
            .impl_predicates
            .get(imp)
            .map_or(&[][..], |p| p)
            .iter()
            .enumerate()
        {
            let Some(next) = self.impl_region_goal_with_proof(
                p,
                block.bound_selfs.get(k).map_or(&[], |t| t),
                block
                    .bound_traits
                    .get(k)
                    .unwrap_or(&TraitRefRegions::default()),
                &lifetimes,
                &args,
                &type_vars,
                Some((env, depth + 1)),
                false,
            ) else {
                return false;
            };
            if !self.impl_prove_super(&next, env, depth + 1) {
                return false;
            }
            proven.push(next);
        }
        for (p, table) in self
            .sigs
            .impl_projections
            .get(imp)
            .map_or(&[][..], |p| p)
            .iter()
            .zip(block.projections.iter())
        {
            if !self.impl_aliases_known(p.ty, &args) {
                return false;
            }
            let Some(expected) = self.instantiate_sig(p.ty, table, &mut { map }, &args, &type_vars)
            else {
                return false;
            };
            let actual_ty = Ty::alias(p.alias).subst(&args);
            // Only the impl header's equalities are assumptions. A selected
            // impl's required equality must also hold after lifetime erasure,
            // including when neither side contributes any region slots.
            let proof = flux_solve::traits::RigidSolver::new(
                self.items,
                self.sigs,
                &self.header_predicates,
                &self.env,
                &self.unsized_params,
            );
            if proof.normalize_ty(actual_ty, None) != proof.normalize_ty(p.ty.subst(&args), None) {
                return false;
            }
            if expected.is_empty() {
                continue;
            }
            let TyKind::Alias(alias) = actual_ty.kind() else {
                return false;
            };
            let alias_vars = match proven.iter().find(|g| {
                g.predicate.trait_ref.trait_ == alias.trait_
                    && alias.args.first() == Some(&g.predicate.self_ty)
                    && alias.args.get(1..) == Some(g.predicate.trait_ref.args)
            }) {
                Some(g) => g.vars.clone(),
                None => {
                    let mut vars = Vec::new();
                    for &ty in p.alias.args {
                        let Some(v) =
                            self.instantiate_sig(ty, &[], &mut { map }, &args, &type_vars)
                        else {
                            return false;
                        };
                        vars.push(v);
                    }
                    vars
                }
            };
            let Some(actual) = self.normalize_regions(alias, &alias_vars, depth + 1) else {
                return false;
            };
            if actual.len() != expected.len() {
                return false;
            }
            for (a, b) in actual.into_iter().zip(expected) {
                self.equate(a, b, Category::Boring, range);
            }
        }
        true
    }

    fn slots(&mut self, ty: Ty) -> u32 {
        if let Some(&n) = self.slot_counts.get(&ty) {
            return n;
        }
        let n = flux_ty::regions::slots(self.items, ty) as u32;
        self.slot_counts.insert(ty, n);
        n
    }

    /// Actual generic arguments can introduce projections absent from the
    /// declaration. Allocate rows for their normalized value types, matching
    /// the input/output rows that instantiate_sig and relate_types consume.
    fn instantiated_type_vars(&mut self, args: &[Ty]) -> Vec<Vec<Var>> {
        args.iter()
            .map(|&ty| {
                let normalized = self.normalize(ty);
                let count = self.slots(normalized);
                self.fresh_list(count)
            })
            .collect()
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

    fn local_vars(&self, local: Local) -> Slots {
        let i = local.index();
        (self.local_base[i]..self.local_base[i + 1]).collect()
    }

    fn normalize(&self, ty: Ty) -> Ty {
        flux_solve::traits::normalize_with_memberships(
            self.items,
            self.sigs,
            &self.env,
            &self.header_predicates,
            ty,
        )
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
        if let (Some(function), Some(regions)) = (body.owner, fn_regions) {
            let predicates = sigs
                .fn_predicates
                .get(function)
                .map_or(&[][..], |rows| rows);
            let bounds: Vec<_> = predicates
                .iter()
                .copied()
                .zip(regions.bound_selfs.iter())
                .zip(regions.bound_traits.iter())
                .map(|((predicate, own), rows)| (predicate, &**own, rows.clone()))
                .collect();
            self.callable_bound_inputs_unambiguous(function, &bounds, TextRange::default());
        }
        let constant_impl =
            body.constant_owner
                .and_then(|konst| match self.items.consts[konst].owner {
                    Some(flux_hir::Owner::Impl(imp)) => Some(imp),
                    _ => None,
                });
        let constant_lifetimes = if let Some(imp) = constant_impl {
            match self.impl_header_environment(imp) {
                Some((lifetimes, _)) => Some(lifetimes),
                None => {
                    self.errors.push(Diagnostic::new(
                        TextRange::default(),
                        "constant owner lifetime metadata is not represented",
                    ));
                    if self.universal.is_empty() {
                        self.new_universal("'static".into());
                    }
                    None
                }
            }
        } else {
            self.new_universal("'static".to_owned());
            None
        };
        let enclosing_regions = body.owner.and_then(|f| sigs.regions.fns.get(f));
        if body.upvars.is_some() {
            self.closure_ext.push(Ext::Static);
        }
        if let Some(r) = enclosing_regions {
            for (i, n) in r.names.iter().enumerate() {
                let name = match n.name {
                    Some(name) => name.to_string(),
                    None => format!("'{}", i + 1),
                };
                self.new_universal(name);
                if body.upvars.is_some() {
                    self.closure_ext.push(Ext::Parent(
                        body.owner.expect("enclosing function"),
                        i as u32,
                    ));
                }
            }
        }
        if body.upvars.is_some()
            && let Some(regions) = enclosing_regions
        {
            for &bound in &regions.outlives {
                if let Outlives::Type(ty, region) = bound {
                    let target = match region {
                        Region::Static => Some(STATIC),
                        Region::Param(index) if (index as usize) < regions.names.len() => {
                            Some(1 + index)
                        }
                        _ => None,
                    };
                    if let Some(target) = target {
                        self.known_types.push((ty, target));
                    }
                }
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
                // The argument's at each call (`Cx::late_output`).
                Region::Late(_) => Some(STATIC),
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
            // Instantiating a selected family can prove caller bounds. Its
            // proof must see the original signature assumptions before any
            // family requirements are generated.
            self.close_known();
            for table in r.params.iter() {
                for &region in table.iter() {
                    if let Region::Param(i) = region
                        && !self.in_params.contains(&(1 + i))
                    {
                        self.in_params.push(1 + i);
                    }
                }
            }
            // What its bounds and its block's say associated types are.
            let mut declared: Vec<(flux_ty::traits::Projection, &[Region])> = Vec::new();
            if let Some(flux_hir::Owner::Impl(imp)) = self.items.functions[f].owner
                && let Some(block) = sigs.regions.impls.get(imp)
            {
                let list = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
                declared.extend(
                    list.iter()
                        .copied()
                        .zip(block.projections.iter().map(|t| &t[..])),
                );
            }
            let list = sigs.fn_projections.get(f).map_or(&[][..], |p| p);
            declared.extend(
                list.iter()
                    .copied()
                    .zip(r.projections.iter().map(|t| &t[..])),
            );
            for (p, table) in declared {
                if self.env_ranked.iter().any(|scheme| scheme.alias == p.alias) {
                    continue;
                }
                if table.iter().any(|r| matches!(r, Region::Late(_))) && !self.is_fn_output(p.alias)
                {
                    self.errors.push(Diagnostic::new(
                        range,
                        "higher-ranked equality input metadata is unavailable",
                    ));
                    continue;
                }
                if table.is_empty() {
                    continue;
                }
                if table.iter().any(|r| matches!(r, Region::Late(_))) {
                    self.env_late.push((p.alias, table.into()));
                }
                let mut map_var = map;
                if let Some(vars) = self.instantiate_sig(p.ty, table, &mut map_var, &[], &[]) {
                    self.env_regions.push((p.alias, vars));
                }
            }
            let sig = &sigs.fns[f];
            let Ok(selected) = self.selected_source_signature(f, range) else {
                return;
            };
            let mut map_var = map;
            for (i, (&ty, regions)) in sig.params.iter().zip(r.params.iter()).enumerate() {
                let (ty, regions) = selected.as_ref().map_or((ty, &**regions), |selected| {
                    (selected.params[i].ty, &*selected.params[i].regions)
                });
                let local = Local::from_raw(i as u32 + 1);
                let vars = self.instantiate_sig(ty, regions, &mut map_var, &[], &[]);
                let vars = if selected.is_some() {
                    let vars = self.selected_local_shape(ty, vars, local, range);
                    if vars.is_none() {
                        return;
                    }
                    vars
                } else {
                    vars
                };
                signature.push((local, vars));
            }
            let vars = if r.opaques.is_empty() {
                let (ty, regions) = selected.as_ref().map_or((sig.ret, &*r.ret), |selected| {
                    (selected.ret.ty, &*selected.ret.regions)
                });
                let vars = self.instantiate_sig(ty, regions, &mut map_var, &[], &[]);
                if selected.is_some() {
                    let vars = self.selected_local_shape(ty, vars, Body::RETURN, range);
                    if vars.is_none() {
                        return;
                    }
                    vars
                } else {
                    vars
                }
            } else {
                let hidden = body.locals[Body::RETURN].ty;
                let mut out = Vec::new();
                self.reveal_sig(f, sig.ret, hidden, &r.ret, &mut 0, &mut out)
                    .then_some(out)
            };
            signature.push((Body::RETURN, vars));
        } else if body.owner.is_none() && body.upvars.is_none() {
            if let Some(konst) = body.constant_owner {
                let lifetimes = constant_lifetimes.as_deref().unwrap_or(&[]);
                let mut map = |r| match r {
                    Region::Static => Some(STATIC),
                    Region::Param(i) => lifetimes.get(i as usize).copied(),
                    _ => None,
                };
                let vars = sigs.regions.consts.get(konst).and_then(|row| {
                    self.instantiate_sig(body.locals[Body::RETURN].ty, row, &mut map, &[], &[])
                });
                signature.push((Body::RETURN, vars));
            } else {
                let n = self.local_vars(Body::RETURN).len();
                signature.push((Body::RETURN, Some(vec![STATIC; n])));
            }
        } else if let Some(upvars) = &body.upvars {
            // Parent slots are already universal before inherited projection
            // rows are instantiated. Capture/argument relations are added below.
            self.close_known();
            // A closure: each lifetime of what it is given is universal,
            // and how they must relate is its creator's to check.
            let by_ref = upvars.kind != flux_ty::ClosureKind::FnOnce;
            // What its creator's `Fn`-family bounds say their results are,
            // where those are the arguments' lifetimes (`F: Fn(&T) ->
            // &U`): its calls of them see the same.
            if let Some(f) = body.owner
                && let Some(r) = sigs.regions.fns.get(f)
            {
                let list = sigs.fn_projections.get(f).map_or(&[][..], |p| p);
                for (p, table) in list.iter().zip(r.projections.iter()) {
                    if !self.is_fn_output(p.alias) {
                        continue;
                    }
                    let late = table.iter().any(|r| matches!(r, Region::Late(_)));
                    let only = table
                        .iter()
                        .all(|r| matches!(r, Region::Late(_) | Region::Static));
                    if !late || !only {
                        continue;
                    }
                    let mut map = |_| Some(STATIC);
                    if let Some(vars) = self.instantiate_sig(p.ty, table, &mut map, &[], &[]) {
                        self.env_regions.push((p.alias, vars));
                        self.env_late.push((p.alias, table.clone()));
                    }
                }
            }
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
                    match p {
                        Outlives::Region(a, b) => self.known_pairs.push((a, b)),
                        // Input/environment references are given well formed.
                        // An inferred return value still owes its own proof.
                        Outlives::Type(ty, region) if i != 0 => self.known_types.push((ty, region)),
                        _ => {}
                    }
                }
            }
        }
        let mut adt_obligations = Vec::new();
        for (local, sig_vars) in signature {
            let vars = self.local_vars(local);
            adt_obligations.push((body.locals[local].ty, vars.clone(), range));
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
        // Unrepresented signature rows and closure parameters may have added
        // universals. Close only their declared/implied assumptions before
        // local annotations or relations can select another family.
        self.close_known();
        // A written local annotation constrains every assignment to its
        // bindings, even if the binding is unused or initialized on a branch.
        for (local, table, range) in &body.region_annotations {
            let vars = self.local_vars(*local);
            for (&v, r) in vars.iter().zip(table.iter()) {
                let fixed = match *r {
                    Region::Static => Some(STATIC),
                    Region::Param(i) => enclosing_regions
                        .and_then(|r| r.names.get(i as usize))
                        .map(|_| 1 + i)
                        .or_else(|| {
                            constant_lifetimes
                                .as_ref()
                                .and_then(|rows| rows.get(i as usize).copied())
                        }),
                    _ => None,
                };
                if let Some(fixed) = fixed {
                    self.equate(v, fixed, Category::Assignment, *range);
                }
            }
            let ty = body.locals[*local].ty;
            adt_obligations.push((ty, vars.clone(), *range));
            let mut implied = Vec::new();
            flux_ty::regions::implied_outlives_with_static(
                self.items,
                sigs,
                ty,
                &vars,
                Some(STATIC),
                &mut implied,
            );
            for constraint in implied {
                match constraint {
                    Outlives::Region(a, b) => self.outlives(a, b, Category::Assignment, *range),
                    Outlives::Type(t, b) => self.type_outlives(t, b, *range),
                }
            }
        }
        for (source, dest, variance, range) in &body.region_relations {
            let (source, from) = self.place_regions(source);
            let (ty, to) = self.place_regions(dest);
            self.relate_types(
                source,
                &from,
                ty,
                &to,
                *variance,
                Category::Assignment,
                *range,
            );
        }
        self.close_known();
        for (ty, vars, range) in adt_obligations {
            self.adt_obligations(ty, &vars, range);
        }
    }

    /// The slots of `hidden`, `f`'s return type `sig_ty` (with slots
    /// `own`) with its opaque types revealed, as `f`'s body has it, into
    /// `out`: those of its opaque types' hidden types are new variables,
    /// which [`Self::member_constraints`] relates to what they capture.
    /// False if the two cannot be matched.
    #[allow(clippy::too_many_arguments)]
    fn reveal_sig(
        &mut self,
        f: flux_hir::FunctionId,
        sig_ty: Ty,
        hidden: Ty,
        own: &[Region],
        next: &mut usize,
        out: &mut Vec<Var>,
    ) -> bool {
        let map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => Some(1 + i),
            Region::Erased | Region::Late(_) => None,
        };
        if !sig_ty.has_opaques() {
            let mut map = map;
            return self.inst(sig_ty, own, next, &mut map, &[], &[] as &[Vec<Var>], out);
        }
        let own_slot = |next: &mut usize, out: &mut Vec<Var>| -> bool {
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
        match (sig_ty.kind(), hidden.kind()) {
            (TyKind::Opaque(o), _) if o.function == f => {
                let mut captures = Vec::new();
                for _ in 0..o.lifetimes {
                    if !own_slot(next, &mut captures) {
                        return false;
                    }
                }
                let regions = self.sigs.regions.fns.get(f);
                let decl = regions.and_then(|r| r.opaques.get(o.index as usize));
                let outlives = decl.map_or(Vec::new(), |d| {
                    d.outlives.iter().filter_map(|&r| map(r)).collect()
                });
                let range = self.items.functions[f]
                    .opaques
                    .get(o.index as usize)
                    .map_or(TextRange::default(), |d| d.range);
                let name = o.name;
                let n = self.slots(hidden);
                let vars = self.fresh_list(n);
                out.extend(&vars);
                self.hidden.push(HiddenSlots {
                    opaque: o,
                    ty: hidden,
                    vars,
                    captures,
                    outlives,
                    range,
                    name,
                });
                true
            }
            (TyKind::Ref(_, a), TyKind::Ref(_, b)) => {
                own_slot(next, out) && self.reveal_sig(f, a, b, own, next, out)
            }
            (TyKind::RawPtr(_, a), TyKind::RawPtr(_, b))
            | (TyKind::Array(a, _), TyKind::Array(b, _))
            | (TyKind::Slice(a), TyKind::Slice(b)) => self.reveal_sig(f, a, b, own, next, out),
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .all(|(&a, &b)| self.reveal_sig(f, a, b, own, next, out)),
            (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) if x.strukt == y.strukt => {
                let n = self.items.structs[x.strukt].generics.lifetimes.len();
                (0..n).all(|_| own_slot(next, out))
                    && xs
                        .iter()
                        .zip(ys)
                        .all(|(&a, &b)| self.reveal_sig(f, a, b, own, next, out))
            }
            (TyKind::Dynamic(a), TyKind::Dynamic(b))
                if a.trait_ == b.trait_ && a.assoc_names == b.assoc_names =>
            {
                (0..flux_ty::regions::object_own_slots(self.items, a)).all(|_| own_slot(next, out))
                    && flux_ty::regions::object_region_types(self.items, a)
                        .zip(flux_ty::regions::object_region_types(self.items, b))
                        .all(|(a, b)| self.reveal_sig(f, a, b, own, next, out))
            }
            _ => false,
        }
    }

    /// What each hidden type of the body's opaque types must outlive, as
    /// rustc's member constraints say: each of its lifetimes one of
    /// those the opaque type captures (`'static` if none), the least
    /// that what flows into it allows; all of its `+ 'a` bounds.
    fn member_constraints(&mut self) {
        if self.hidden.is_empty() {
            return;
        }
        // What each variable is outlived by, through the constraints so
        // far: the universal regions above it.
        let n = self.vars as usize;
        let by_sub = Adjacency::new(n, self.constraints.iter().map(|c| (c.sub, c.sup)));
        let hidden = std::mem::take(&mut self.hidden);
        let mut uncaptured = false;
        // Where rustc blames what the hidden type must outlive: the
        // value returned.
        let returned = self.return_range();
        for h in &hidden {
            let h = &HiddenSlots {
                range: returned.unwrap_or(h.range),
                vars: h.vars.clone(),
                captures: h.captures.clone(),
                outlives: h.outlives.clone(),
                ..*h
            };
            self.hidden_projections(h);
            for &b in &h.outlives {
                self.stored_type_outlives(h.ty, &h.vars, b, Category::OpaqueType, h.range);
            }
            for &v in &h.vars {
                if !h.outlives.is_empty() {
                    for &b in &h.outlives {
                        self.outlives(v, b, Category::OpaqueType, h.range);
                    }
                    continue;
                }
                let mut above = Vec::new();
                let mut seen = BitSet::new_empty(n);
                let mut stack = vec![v];
                seen.insert(v as usize);
                while let Some(x) = stack.pop() {
                    if let Some(&u) = self.universal_of.get(&x) {
                        above.push(u);
                    }
                    for &sup in &by_sub[x as usize] {
                        if !seen.contains(sup as usize) {
                            seen.insert(sup as usize);
                            stack.push(sup);
                        }
                    }
                }
                // One of the function's lifetimes that it does not
                // capture: rustc's E0700.
                if above.iter().any(|&u| {
                    let var = self.universal[u as usize].0;
                    var != STATIC && !h.captures.contains(&var) && self.closure_ext.is_empty()
                }) {
                    if !uncaptured {
                        uncaptured = true;
                        self.errors.push(Diagnostic::new(
                            h.range,
                            format!(
                                "hidden type for `{}` captures lifetime that does not appear in bounds",
                                h.name
                            ),
                        ));
                    }
                    continue;
                }
                let Some(&first) = h.captures.first() else {
                    self.outlives(v, STATIC, Category::OpaqueType, h.range);
                    continue;
                };
                let index = |c: Var| self.universal_of[&c];
                let fits = |c: Var| above.iter().all(|&u| self.known(u, index(c)));
                let fitting: Vec<Var> = h.captures.iter().copied().filter(|&c| fits(c)).collect();
                // The least: one every other that fits outlives.
                let chosen = fitting
                    .iter()
                    .copied()
                    .find(|&c| fitting.iter().all(|&d| self.known(index(d), index(c))))
                    .or(fitting.first().copied())
                    .unwrap_or(first);
                self.outlives(v, chosen, Category::OpaqueType, h.range);
            }
        }
        self.hidden = hidden;
    }

    /// Rows are tied to the directly resolved written bound, not to an
    /// elaborated predicate index. Refuse incomplete metadata before proving
    /// either the hidden type or a caller's opaque obligation.
    fn opaque_bound_metadata(&self, opaque: flux_ty::OpaqueTy) -> bool {
        let Some(sig) = self
            .sigs
            .opaques
            .get(opaque.function)
            .and_then(|s| s.get(opaque.index as usize))
        else {
            return false;
        };
        let Some(owner) = self.sigs.regions.fns.get(opaque.function) else {
            return false;
        };
        let Some(decl) = owner.opaques.get(opaque.index as usize) else {
            return false;
        };
        let Some(written) = self.items.functions[opaque.function]
            .opaques
            .get(opaque.index as usize)
        else {
            return false;
        };
        if decl.bounds.len() != sig.predicates.len()
            || sig.bound_origins.len() != sig.predicates.len()
            || decl.projections.len() != sig.projections.len()
            || decl.captures.len() != opaque.lifetimes as usize
            || sig.bound_origins.windows(2).any(|pair| pair[0] >= pair[1])
        {
            return false;
        }
        let mut projection_origins = Vec::new();
        for (index, &origin) in sig.bound_origins.iter().enumerate() {
            let Some(source) = written.bounds.get(origin as usize) else {
                return false;
            };
            let last = source.path.segments.len().saturating_sub(1);
            for binding in source
                .path
                .args(last)
                .into_iter()
                .flat_map(|args| &args.bindings)
            {
                if sig
                    .projections
                    .get(projection_origins.len())
                    .is_none_or(|p| p.alias.name != binding.name)
                {
                    return false;
                }
                projection_origins.push(index);
            }
        }
        if projection_origins.len() != sig.projections.len() {
            return false;
        }
        for (index, (projection, table)) in
            sig.projections.iter().zip(&decl.projections).enumerate()
        {
            let late = if flux_ty::fn_trait_kind(self.items, projection.alias.trait_).is_some() {
                projection
                    .alias
                    .args
                    .get(1)
                    .map_or(0, |&ty| flux_ty::regions::slots(self.items, ty))
            } else {
                decl.bounds[projection_origins[index]].regions.bound as usize
            };
            if table.len() != flux_ty::regions::slots(self.items, projection.ty)
                || table.iter().any(|&region| match region {
                    Region::Param(i) => {
                        i as usize >= owner.names.len() || !decl.captures.contains(&region)
                    }
                    Region::Late(i) => i as usize >= late,
                    Region::Erased => true,
                    Region::Static => false,
                })
            {
                return false;
            }
        }
        for ((bound, predicate), origin) in decl
            .bounds
            .iter()
            .zip(sig.predicates.iter())
            .zip(sig.bound_origins.iter())
        {
            let Some(source) = written.bounds.get(*origin as usize) else {
                return false;
            };
            if bound.predicate != *predicate
                || bound.origin != *origin
                || self
                    .items
                    .resolve_trait(self.items.functions[opaque.function].loc.module, source)
                    .0
                    != Some(predicate.trait_ref.trait_)
                || bound.regions.args.len() != predicate.trait_ref.args.len()
                || bound.regions.lifetimes.len()
                    != self.items.traits[predicate.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .len()
                || bound.regions.bound != source.bound_lifetimes.len() as u32
                || bound
                    .regions
                    .args
                    .iter()
                    .zip(predicate.trait_ref.args)
                    .any(|(row, &ty)| row.len() != flux_ty::regions::slots(self.items, ty))
            {
                return false;
            }
            for &region in bound
                .regions
                .lifetimes
                .iter()
                .chain(bound.regions.args.iter().flatten())
            {
                match region {
                    Region::Param(i)
                        if i as usize >= owner.names.len() || !decl.captures.contains(&region) =>
                    {
                        return false;
                    }
                    Region::Late(i)
                        if (bound.regions.bound != 0 && i >= bound.regions.bound)
                            || (bound.regions.bound == 0
                                && flux_ty::fn_trait_kind(
                                    self.items,
                                    predicate.trait_ref.trait_,
                                )
                                .is_none()) =>
                    {
                        return false;
                    }
                    Region::Erased
                        if flux_ty::fn_trait_kind(self.items, predicate.trait_ref.trait_)
                            .is_none() =>
                    {
                        return false;
                    }
                    _ => {}
                }
            }
        }
        true
    }

    /// What the bounds of `h`'s opaque type say its associated types are,
    /// with lifetimes (`impl Iterator<Item = &'a u32>`), its hidden
    /// type's must outlive: what callers are told they are. Where flux
    /// cannot tell the hidden type's (an adapter's closure decides it),
    /// each lifetime of the hidden type must outlive each of them.
    fn hidden_projections(&mut self, h: &HiddenSlots) {
        let sigs = self.sigs;
        let o = h.opaque;
        let (Some(sig), Some(decl)) = (
            sigs.opaques
                .get(o.function)
                .and_then(|s| s.get(o.index as usize)),
            sigs.regions
                .fns
                .get(o.function)
                .and_then(|r| r.opaques.get(o.index as usize)),
        ) else {
            return;
        };
        let mut args = o.args.to_vec();
        args.push(h.ty);
        let map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => Some(1 + i),
            Region::Erased | Region::Late(_) => None,
        };
        if !self.opaque_bound_metadata(o) {
            self.errors
                .push(Diagnostic::new(h.range, "invalid opaque bound metadata"));
            return;
        }
        {
            let lifetimes: Vec<_> =
                (1..=self.sigs.regions.fns[o.function].names.len() as u32).collect();
            let mut arg_vars: Vec<_> = o
                .args
                .iter()
                .map(|&t| self.bound_slots(t, &[], &[]).unwrap_or_default())
                .collect();
            arg_vars.push(h.vars.clone());
            let previous = std::mem::replace(
                &mut self.call_projections,
                sig.projections
                    .iter()
                    .copied()
                    .zip(decl.projections.iter().cloned())
                    .collect(),
            );
            let bounds: Vec<_> = decl
                .bounds
                .iter()
                .map(|bound| (bound.predicate, &[][..], bound.regions.clone()))
                .collect();
            let written = &self.items.functions[o.function].opaques[o.index as usize];
            if sig
                .predicates
                .iter()
                .zip(&sig.bound_origins)
                .any(|(&predicate, &origin)| {
                    written.bounds.get(origin as usize).is_none_or(|bound| {
                        !self.callable_output_recipe_present(bound, predicate, &sig.projections)
                    })
                })
            {
                self.errors.push(Diagnostic::new(
                    h.range,
                    "callable output metadata is incomplete",
                ));
                self.call_projections = previous;
                return;
            }
            if !self.callable_bound_inputs_unambiguous(o.function, &bounds, h.range) {
                self.call_projections = previous;
                return;
            }
            for bound in &decl.bounds {
                self.prove_bound(
                    bound.predicate,
                    &[],
                    &bound.regions,
                    &lifetimes,
                    &args,
                    &arg_vars,
                    h.range,
                    0,
                );
            }
            self.call_projections = previous;
            let written_bounds: Vec<_> = sig
                .bound_origins
                .iter()
                .filter_map(|&origin| written.bounds.get(origin as usize))
                .collect();
            for (index, (p, table)) in sig
                .projections
                .iter()
                .zip(decl.projections.iter())
                .enumerate()
            {
                if table.iter().any(|r| matches!(r, Region::Late(_))) {
                    let input = self.written_projection_input(
                        &written_bounds,
                        &sig.projections,
                        index,
                        &bounds,
                        h.range,
                    );
                    self.prove_projection(
                        *p,
                        table,
                        input.as_ref(),
                        &lifetimes,
                        &args,
                        &arg_vars,
                        h.range,
                    );
                }
            }
        }
        for (p, table) in sig.projections.iter().zip(decl.projections.iter()) {
            if table.is_empty() {
                continue;
            }
            let mut map = map;
            let Some(declared) = self.instantiate_sig(p.ty, table, &mut map, &[], &[]) else {
                continue;
            };
            let alias = p.subst(&args).alias;
            let mut alias_vars = vec![h.vars.clone()];
            for &a in &alias.args[1..] {
                let mut vars = Vec::new();
                let mut none = |_| None;
                self.inst(
                    a,
                    &[],
                    &mut 0,
                    &mut none,
                    &[],
                    &[] as &[Vec<Var>],
                    &mut vars,
                );
                alias_vars.push(vars);
            }
            match self.normalize_regions(alias, &alias_vars, 0) {
                Some(found) if found.len() == declared.len() => {
                    self.relate(
                        p.ty,
                        &found,
                        &declared,
                        Variance::Covariant,
                        Category::OpaqueType,
                        h.range,
                    );
                }
                _ => {
                    for &v in &h.vars {
                        for &d in &declared {
                            self.outlives(v, d, Category::OpaqueType, h.range);
                        }
                    }
                }
            }
        }
    }

    /// Where the body's return value is made: the first assignment or
    /// call into `_0`.
    fn return_range(&self) -> Option<TextRange> {
        let is_return = |p: &Place| p.local == Body::RETURN && p.projection.is_empty();
        self.body.blocks.values().find_map(|data| {
            let assigned = data.statements.iter().find_map(|s| match &s.kind {
                StatementKind::Assign(p, _) if is_return(p) => Some(s.range),
                _ => None,
            });
            assigned.or(match &data.terminator.kind {
                TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. }
                    if is_return(dest) =>
                {
                    Some(data.terminator.range)
                }
                _ => None,
            })
        })
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
    fn inst<A: AsRef<[Var]>, O: Extend<Var>>(
        &mut self,
        ty: Ty,
        own: &[Region],
        next: &mut usize,
        map: &mut dyn FnMut(Region) -> Option<Var>,
        args: &[Ty],
        arg_vars: &[A],
        out: &mut O,
    ) -> bool {
        let mut own_slot = |next: &mut usize, out: &mut O| -> bool {
            let r = own.get(*next).copied().unwrap_or(Region::Erased);
            *next += 1;
            match map(r) {
                Some(v) => {
                    out.extend([v]);
                    true
                }
                None => false,
            }
        };
        match ty.kind() {
            TyKind::Param(p) => {
                if let Some(vars) = arg_vars.get(p.index as usize) {
                    out.extend(vars.as_ref().iter().copied());
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
            TyKind::Opaque(opaque) => {
                (0..opaque.lifetimes).all(|_| own_slot(next, out))
                    && opaque
                        .args
                        .iter()
                        .all(|&t| self.inst(t, own, next, map, args, arg_vars, out))
            }
            TyKind::Dynamic(object) => {
                (0..flux_ty::regions::object_own_slots(self.items, object))
                    .all(|_| own_slot(next, out))
                    && flux_ty::regions::object_region_types(self.items, object)
                        .all(|ty| self.inst(ty, own, next, map, args, arg_vars, out))
            }
            TyKind::FnPtr(sig) => {
                let Some(r) = sig.regions else {
                    return true;
                };
                for _ in 0..r.free {
                    if !own_slot(next, out) {
                        return false;
                    }
                }
                r.types
                    .iter()
                    .all(|&t| self.inst(t, own, next, map, args, arg_vars, out))
            }
            TyKind::Alias(alias) if flux_ty::regions::has_projection_inputs(self.items, alias) => {
                match self.instantiate_family(alias, own, next, map, args, arg_vars) {
                    Some(vars) => {
                        out.extend(vars);
                        true
                    }
                    None => false,
                }
            }
            TyKind::Alias(alias) => {
                let instance = if args.is_empty() { ty } else { ty.subst(args) };
                let normalized = self.normalize(instance);
                if self.slots(normalized) == 0 {
                    return true;
                }
                // A bound's result with its own lifetimes is the call's
                // arguments', below.
                if let TyKind::Alias(a) = instance.kind()
                    && !self.env_late.iter().any(|(e, _)| *e == a)
                    && let Some((_, vars)) = self.env_regions.iter().find(|(e, _)| *e == a)
                {
                    out.extend(vars.iter().copied());
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
                // Substitution can introduce captured inputs into a legacy
                // parameter-only alias, such as Iterator::Item for an abstract
                // IntoIterator::IntoIter. Its substituted occurrence uses the
                // same projection path as an explicitly written reference.
                let output = if flux_ty::regions::has_projection_inputs(self.items, instance) {
                    self.ordinary_projection_output(instance, &[], &alias_vars)
                } else {
                    self.normalize_regions(instance, &alias_vars, 0)
                };
                match output {
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
        if let Some(scheme) = self.env_ranked.iter().find(|s| s.alias == alias).cloned() {
            return self.ranked_projection_output(&scheme, &[], arg_vars);
        }
        if depth > 16 {
            return None;
        }
        if let Some((_, vars)) = self.env_regions.iter().find(|(e, _)| *e == alias) {
            let mut vars = vars.clone();
            // A bound's result with its own lifetimes: the argument's.
            if let Some((_, table)) = self.env_late.iter().find(|(e, _)| *e == alias)
                && table.len() == vars.len()
            {
                let inputs = arg_vars.get(1).map_or(&[][..], Vec::as_slice);
                for (v, r) in vars.iter_mut().zip(table.iter()) {
                    if let Region::Late(j) = *r
                        && let Some(&input) = inputs.get(j as usize)
                    {
                        *v = input;
                    }
                }
            }
            return Some(vars);
        }
        if let TyKind::Dynamic(object) = alias.args[0].kind() {
            if arg_vars.first()?.len() != self.slots(alias.args[0]) as usize {
                return None;
            }
            let projection =
                flux_solve::traits::dyn_projections(self.items, self.sigs, alias.args[0], object)
                    .into_iter()
                    .find(|projection| projection.alias == alias)?;
            let row = flux_ty::regions::object_associated_range(
                self.items,
                object,
                projection.alias.name,
            )?;
            return arg_vars.first()?.get(row).map(<[Var]>::to_vec);
        }
        if let Some(output) = self.family_projection_output(alias, arg_vars, &[]) {
            return Some(output);
        }
        // An associated type's declaration can forward another projection:
        // IntoIterator::IntoIter: Iterator<Item = Self::Item>. Preserve the
        // caller's Item regions through that equality instead of tying them
        // to the temporary reference used to call Iterator::next.
        if let TyKind::Alias(owner) = alias.args[0].kind() {
            let implied: Vec<_> = self
                .sigs
                .traits
                .get(owner.trait_)
                .into_iter()
                .flat_map(|sig| sig.implied.iter())
                .map(|p| p.subst(owner.args))
                .filter(|p| p.alias == alias)
                .collect();
            let mut forwarded = None;
            for projection in implied {
                if let TyKind::Alias(source) = projection.ty.kind()
                    && source != alias
                    && !self.env_late.iter().any(|(e, _)| *e == source)
                    && let Some((_, vars)) = self.env_regions.iter().find(|(e, _)| *e == source)
                {
                    if forwarded.as_ref().is_some_and(|previous| previous != vars) {
                        return None;
                    }
                    forwarded = Some(vars.clone());
                }
            }
            if forwarded.is_some() {
                return forwarded;
            }
        }
        // What an opaque type's bounds say, or else what an `impl`
        // block for it does (`impl<I: Iterator> IntoIterator for I`).
        if let TyKind::Opaque(opaque) = alias.args[0].kind()
            && let Some(vars) = self.opaque_projection(
                opaque,
                alias,
                arg_vars.first()?,
                arg_vars.get(1).map_or(&[][..], Vec::as_slice),
            )
        {
            return Some(vars);
        }
        if let TyKind::Closure(id, list) = alias.args[0].kind()
            && self.is_fn_output(alias)
        {
            let inputs = arg_vars.get(1).map_or(&[][..], Vec::as_slice);
            return Some(self.closure_output(id, list, arg_vars.first()?, inputs));
        }
        if self.is_fn_output(alias) {
            let inputs = arg_vars.get(1)?;
            if let TyKind::FnDef(_) = alias.args[0].kind() {
                return self.function_output(alias.args[0], inputs);
            }
            if let TyKind::FnPtr(sig) = alias.args[0].kind() {
                if sig.regions.is_some() {
                    let mut at = 0;
                    let mut rows = Vec::new();
                    let input = self.normalize(*alias.args.get(1)?);
                    let TyKind::Tuple(input_types) = input.kind() else {
                        return None;
                    };
                    for &ty in input_types {
                        let n = self.slots(ty) as usize;
                        rows.push(inputs.get(at..at + n)?.to_vec());
                        at += n;
                    }
                    let own = arg_vars.first()?;
                    return self.pointer_inputs(sig, own, input_types, &rows, self.call_range);
                }
                if self.slots(sig.ret()) != 0 {
                    return None;
                }
                return Some(Vec::new());
            }
        }
        // Recursive instantiation above produced the rows of the selected
        // argument values. Match those rows against their normalized types,
        // rather than a nested projection's zero-slot declaration spelling.
        let inputs: Vec<_> = alias.args.iter().map(|&ty| self.normalize(ty)).collect();
        if inputs.len() != arg_vars.len()
            || inputs
                .iter()
                .zip(arg_vars)
                .any(|(&ty, row)| row.len() != self.slots(ty) as usize)
        {
            return None;
        }
        let (items, sigs) = (self.items, self.sigs);
        let goal = flux_ty::Predicate {
            self_ty: inputs[0],
            trait_ref: flux_ty::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: Ty::list(&inputs[1..]),
            },
        };
        let selected = match flux_solve::traits::select_impl(items, sigs, goal) {
            Some(selected) => Some(selected),
            None => {
                // A rigid caller parameter can select a blanket impl only
                // through its original caller bounds. Header matching alone
                // does not establish the selected impl's conditions.
                let candidate = self.matching_impl(goal)?;
                let known = if self.body.owner.is_none() {
                    self.family_goals.clone()
                } else {
                    self.caller_region_goals()
                };
                let requirement = ImplRegionGoal {
                    predicate: goal,
                    vars: arg_vars.to_vec(),
                    lifetimes: Vec::new(),
                    bound: Vec::new(),
                };
                if !self.impl_prove_super(&requirement, &known, depth) {
                    return None;
                }
                Some(candidate)
            }
        };
        if let Some((imp, impl_args)) = selected {
            let value = flux_solve::traits::impl_type(sigs, imp, alias.name)?;
            let regions = sigs.regions.impls.get(imp)?;
            let ImplBinding { lifetimes, types } = self.bind_impl(
                imp,
                &impl_args,
                &inputs,
                arg_vars,
                &[],
                TextRange::default(),
                depth,
                None,
            )?;
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
                Region::Erased | Region::Late(_) => None,
            };
            return self.instantiate_sig(value, &table, &mut map, &impl_args, &type_vars);
        }
        None
    }

    /// A function item's output, instantiated from the same signature as
    /// a direct call. It may borrow its arguments, never an adapter that
    /// stores it; invariant slots must stay equal to the inputs' slots.
    fn function_output(&mut self, ty: Ty, inputs: &[Var]) -> Option<Vec<Var>> {
        let TyKind::FnDef(def) = ty.kind() else {
            return None;
        };
        self.function_output_typed(ty, def.sig.params(), inputs)
    }

    fn function_output_typed(
        &mut self,
        ty: Ty,
        input_types: &[Ty],
        inputs: &[Var],
    ) -> Option<Vec<Var>> {
        self.function_output_with_obligations(ty, input_types, inputs, FunctionUse::Call, None)
    }

    /// Instantiate the item's bounds even when its address exposes no callable
    /// region recipe. Type arguments absent from the ABI still constrain it.
    fn item_instantiation(
        &mut self,
        function: flux_hir::FunctionId,
        args: &[Ty],
        written: Option<&flux_ty::regions::TypeArgRegions>,
        use_: FunctionUse,
        range: TextRange,
    ) -> Option<(Vec<Var>, Vec<Vec<Var>>)> {
        let count = self.sigs.regions.fns.get(function)?.names.len();
        let lifetimes = self.fresh_list(count as u32);
        let type_vars = self.instantiated_type_vars(args);
        if let Some(written) = written {
            for (index, row) in written {
                let vars = type_vars.get(*index as usize)?;
                if vars.len() != row.len() {
                    return None;
                }
                for (&var, region) in vars.iter().zip(row.iter()) {
                    let declared = match *region {
                        Region::Static => Some(STATIC),
                        Region::Param(i) if self.universal_of.contains_key(&(1 + i)) => Some(1 + i),
                        Region::Erased => None,
                        Region::Param(_) | Region::Late(_) => return None,
                    };
                    if let Some(declared) = declared {
                        self.equate(var, declared, Category::CallArgument, range);
                    }
                }
            }
        }
        // Enforce the owner contract even if a malformed input/output table
        // makes normalization fall back before it can produce output slots.
        self.signature_obligations_for(function, &lifetimes, args, &type_vars, range, use_);
        Some((lifetimes, type_vars))
    }

    fn function_output_with_obligations(
        &mut self,
        ty: Ty,
        input_types: &[Ty],
        inputs: &[Var],
        use_: FunctionUse,
        written: Option<&flux_ty::regions::TypeArgRegions>,
    ) -> Option<Vec<Var>> {
        let TyKind::FnDef(def) = ty.kind() else {
            return None;
        };
        if input_types.len() != def.sig.params().len() {
            return None;
        }
        let sigs = self.sigs;
        let sig = &sigs.fns[def.function];
        let Some(regions) = sigs.regions.fns.get(def.function) else {
            self.missing_trait_owner_regions(def.function, self.call_range);
            return None;
        };
        let range = self.call_range;
        let (lifetimes, type_vars) =
            self.item_instantiation(def.function, def.args, written, use_, range)?;
        let mut map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        let selected = self.selected_source_signature(def.function, range).ok()?;
        let mut at = 0;
        for (i, &ty) in def.sig.params().iter().enumerate() {
            let source = if selected.is_some() {
                self.normalize(input_types[i])
            } else {
                input_types[i]
            };
            let n = self.slots(source) as usize;
            let actual = inputs.get(at..at + n)?;
            at += n;
            let (formal_ty, table) = selected.as_ref().map_or(
                (*sig.params.get(i)?, &**regions.params.get(i)?),
                |selected| (selected.params[i].ty, &*selected.params[i].regions),
            );
            let formal = self.instantiate_sig(formal_ty, table, &mut map, def.args, &type_vars);
            let formal = if selected.is_some() {
                self.selected_value_shape(formal_ty.subst(def.args), formal, ty, range)?
            } else {
                formal?
            };
            self.relate_types(
                source,
                actual,
                ty,
                &formal,
                if use_ == FunctionUse::CallableEquality {
                    Variance::Invariant
                } else {
                    Variance::Covariant
                },
                Category::CallArgument,
                range,
            );
        }
        if use_ == FunctionUse::CallableEquality
            && type_vars.iter().flatten().any(|&fixed| {
                inputs.iter().any(|&input| {
                    self.holes.contains(&input) && self.callable_output_equivalent(fixed, input)
                })
            })
        {
            self.errors.push(Diagnostic::new(
                range,
                "function item type arguments cannot bind universal callback inputs",
            ));
            return None;
        }
        if selected.is_some() && at != inputs.len() {
            self.errors.push(Diagnostic::new(
                range,
                "selected method argument region rows do not match",
            ));
            return None;
        }
        let (ret, table) = selected
            .as_ref()
            .map_or((sig.ret, &*regions.ret), |selected| {
                (selected.ret.ty, &*selected.ret.regions)
            });
        let output = self.instantiate_sig(ret, table, &mut map, def.args, &type_vars);
        if selected.is_some() {
            self.selected_value_shape(ret.subst(def.args), output, def.sig.ret(), range)
        } else {
            output
        }
    }

    /// The slots of what `opaque`'s bounds say `alias` (of it) is, where
    /// `opaque`'s own slots are `vars`: its lifetimes are those it
    /// captures, which its bounds may name (`impl Iterator<Item = &'a
    /// u32>`).
    fn opaque_projection(
        &mut self,
        opaque: flux_ty::OpaqueTy,
        alias: AliasTy,
        vars: &[Var],
        inputs: &[Var],
    ) -> Option<Vec<Var>> {
        let sigs = self.sigs;
        let sig = sigs
            .opaques
            .get(opaque.function)?
            .get(opaque.index as usize)?;
        let args = flux_solve::traits::opaque_args(opaque);
        let alias = alias.map_args(|ty| self.normalize(ty));
        let j = sig
            .projections
            .iter()
            .position(|p| p.subst(&args).alias.map_args(|ty| self.normalize(ty)) == alias)?;
        self.opaque_projection_at(opaque, j, vars, inputs, None)
    }

    /// Instantiate one original opaque output occurrence after its owning
    /// predicate has supplied the input/owner rows used for selection.
    fn opaque_projection_at(
        &mut self,
        opaque: flux_ty::OpaqueTy,
        index: usize,
        vars: &[Var],
        inputs: &[Var],
        quantified: Option<&[Option<Var>]>,
    ) -> Option<Vec<Var>> {
        let sig = self
            .sigs
            .opaques
            .get(opaque.function)?
            .get(opaque.index as usize)?;
        let regions = self.sigs.regions.fns.get(opaque.function)?;
        let decl = regions.opaques.get(opaque.index as usize)?;
        let j = index;
        let table = decl.projections.get(j)?;
        let captures = &decl.captures;
        // Its arguments' slots follow its own.
        let mut next = captures.len();
        let mut type_vars = Vec::with_capacity(opaque.args.len());
        for &a in opaque.args {
            let n = self.slots(a) as usize;
            type_vars.push(vars.get(next..next + n)?.to_vec());
            next += n;
        }
        let late = sig.projections[j]
            .alias
            .args
            .get(1)
            .map_or(Vec::new(), |&ty| {
                self.late_slots(ty, inputs, opaque.args, &type_vars)
            });
        let mut map = |r: Region| match r {
            Region::Static => Some(STATIC),
            r @ Region::Param(_) => {
                let at = captures.iter().position(|&c| c == r)?;
                vars.get(at).copied()
            }
            Region::Late(i) => match quantified {
                Some(row) => row.get(i as usize).copied().flatten(),
                None => late.get(i as usize).copied().filter(|&v| v != Var::MAX),
            },
            Region::Erased => None,
        };
        let ty = sig.projections[j].ty;
        self.instantiate_sig(ty, table, &mut map, opaque.args, &type_vars)
    }

    /// Matches `impl` block `imp`'s header, with generic arguments
    /// `impl_args`, to `header` (`Self` and the trait's arguments), whose
    /// slots are `header_vars`: binds the block's lifetimes and its type
    /// parameters' slots. Trait selection ignores lifetimes, so the block
    /// holds only where they are what its header says: a lifetime or type
    /// it names twice must be the same each time (`impl<'a> PartialEq for
    /// Token<'a>`, whose `Rhs` is `Token<'a>` too), and one it names as
    /// `'static` must be `'static`, as rustc's matching equates them.
    /// The trait's explicit lifetime arguments are invariant too, and bind
    /// the same impl parameters as its self type and type arguments.
    #[allow(clippy::too_many_arguments)]
    fn bind_impl(
        &mut self,
        imp: flux_hir::ImplId,
        impl_args: &[Ty],
        header: &[Ty],
        header_vars: &[Vec<Var>],
        trait_lifetimes: &[Var],
        range: TextRange,
        depth: u32,
        declaration_env: Option<&[ImplRegionGoal]>,
    ) -> Option<ImplBinding> {
        let sigs = self.sigs;
        let regions = sigs.regions.impls.get(imp)?;
        let mut binding = ImplBinding {
            lifetimes: vec![None; regions.names.len()],
            types: vec![None; impl_args.len()],
        };
        let mut eqs = Vec::new();
        if regions.trait_lifetimes.len() != trait_lifetimes.len() {
            return None;
        }
        for (&region, &actual) in regions.trait_lifetimes.iter().zip(trait_lifetimes) {
            match region {
                Region::Param(i) => {
                    let slot = binding.lifetimes.get_mut(i as usize)?;
                    match *slot {
                        Some(previous) => eqs.push((previous, actual)),
                        None => *slot = Some(actual),
                    }
                }
                Region::Static => eqs.push((STATIC, actual)),
                Region::Erased => {}
                Region::Late(_) => return None,
            }
        }
        let trait_args = sigs.impl_traits.get(imp).map_or(&[][..], |t| t.args);
        let patterns = std::iter::once(sigs.impls[imp]).chain(trait_args.iter().copied());
        let tables = std::iter::once(&regions.self_ty).chain(regions.trait_args.iter());
        for (k, (pattern, table)) in patterns.zip(tables).enumerate() {
            let (Some(&actual), Some(vars)) = (header.get(k), header_vars.get(k)) else {
                return None;
            };
            if matches!(pattern.kind(), TyKind::Alias(_))
                && !matches!(actual.kind(), TyKind::Alias(_))
                && let Some(env) = declaration_env
            {
                // Selecting an impl instantiates its declared lifetime
                // binders existentially. Reuse earlier header bindings; a
                // projection may constrain a fresh lifetime, never infer an
                // unbound type parameter from its output.
                for lifetime in &mut binding.lifetimes {
                    if lifetime.is_none() {
                        *lifetime = Some(self.fresh());
                    }
                }
                let (output, output_rows) = self.projected_header_rows(
                    pattern,
                    regions.header_selections.get(k)?,
                    impl_args,
                    &binding,
                    env,
                    depth,
                )?;
                if output != self.normalize(actual)
                    || output_rows.len() != vars.len()
                    || !flux_solve::fn_regions::same_binders(self.items, output, actual)
                {
                    return None;
                }
                eqs.extend(output_rows.into_iter().zip(vars.iter().copied()));
                continue;
            }
            if !flux_solve::fn_regions::same_binders(self.items, pattern.subst(impl_args), actual) {
                return None;
            }
            let (mut t, mut v) = (0, 0);
            if !self.match_at(
                pattern,
                table,
                &mut t,
                actual,
                vars,
                &mut v,
                &mut binding,
                &mut eqs,
            ) {
                return None;
            }
        }
        // What its bounds say associated types are, which may name its
        // lifetimes and parameters too (`impl<'a, I: Iterator<Item = &'a
        // T>, T>`): those of the types the header binds.
        let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
        for (p, table) in projections.iter().zip(regions.projections.iter()) {
            let instance = Ty::alias(p.alias).subst(impl_args);
            let TyKind::Alias(alias) = instance.kind() else {
                continue;
            };
            // A closure's `Output` binds the parameters it is, which
            // its header may not name (`B` of `impl<B, I, F: FnMut(I::Item)
            // -> B> Iterator for Map<I, F>`).
            if table.is_empty() && !self.is_callable_output(alias) && p.alias.family_args.is_empty()
            {
                continue;
            }
            let type_vars: Vec<Vec<Var>> = binding
                .types
                .iter()
                .map(|t| t.clone().unwrap_or_default())
                .collect();
            let mut alias_vars = Vec::with_capacity(p.alias.args.len());
            for &a in p.alias.args {
                match self.bound_slots(a, impl_args, &type_vars) {
                    Some(vars) => alias_vars.push(vars),
                    None => break,
                }
            }
            if alias_vars.len() != p.alias.args.len() {
                continue;
            }
            let actual = if p.alias.family_args.is_empty() {
                self.normalize_regions(alias, &alias_vars, depth + 1)
            } else {
                self.instantiate_sig(
                    Ty::alias(p.alias),
                    &[],
                    &mut |_| None,
                    impl_args,
                    &type_vars,
                )
            };
            let Some(actual) = actual else {
                continue;
            };
            let actual_ty = self.normalize(instance);
            let (mut t, mut v) = (0, 0);
            self.match_at(
                p.ty,
                table,
                &mut t,
                actual_ty,
                &actual,
                &mut v,
                &mut binding,
                &mut eqs,
            );
        }
        for (a, b) in eqs {
            self.equate(a, b, self.proving, range);
        }
        Some(binding)
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
        binding: &mut ImplBinding,
        eqs: &mut Vec<(Var, Var)>,
    ) -> bool {
        let slot = |t: &mut usize,
                    v: &mut usize,
                    binding: &mut ImplBinding,
                    eqs: &mut Vec<(Var, Var)>|
         -> bool {
            let (r, var) = (table.get(*t).copied(), vars.get(*v).copied());
            *t += 1;
            *v += 1;
            match (r, var) {
                (Some(Region::Param(i)), Some(var)) => {
                    match binding.lifetimes.get_mut(i as usize) {
                        Some(slot @ None) => {
                            *slot = Some(var);
                            true
                        }
                        Some(Some(bound)) => {
                            eqs.push((*bound, var));
                            true
                        }
                        None => false,
                    }
                }
                (Some(Region::Static), Some(var)) => {
                    eqs.push((STATIC, var));
                    true
                }
                (Some(Region::Erased), Some(_)) => true,
                (Some(Region::Late(i)), Some(var)) => var == Var::MAX - i,
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
                if own.iter().any(|&var| var >= self.vars) {
                    return false;
                }
                match binding.types.get_mut(p.index as usize) {
                    Some(slot @ None) => {
                        *slot = Some(own.to_vec());
                        true
                    }
                    Some(Some(bound)) if bound.len() == own.len() => {
                        eqs.extend(bound.iter().copied().zip(own.iter().copied()));
                        true
                    }
                    _ => false,
                }
            }
            (TyKind::Ref(_, a), TyKind::Ref(_, b)) => {
                slot(t, v, binding, eqs) && self.match_at(a, table, t, b, vars, v, binding, eqs)
            }
            (TyKind::RawPtr(_, a), TyKind::RawPtr(_, b))
            | (TyKind::Array(a, _), TyKind::Array(b, _))
            | (TyKind::Slice(a), TyKind::Slice(b)) => {
                self.match_at(a, table, t, b, vars, v, binding, eqs)
            }
            (TyKind::Tuple(xs), TyKind::Tuple(ys)) if xs.len() == ys.len() => xs
                .iter()
                .zip(ys)
                .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, binding, eqs)),
            (TyKind::Adt(x, xs), TyKind::Adt(y, ys)) if x.strukt == y.strukt => {
                let n = self.items.structs[x.strukt].generics.lifetimes.len();
                (0..n).all(|_| slot(t, v, binding, eqs))
                    && xs
                        .iter()
                        .zip(ys)
                        .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, binding, eqs))
            }
            (TyKind::Opaque(x), TyKind::Opaque(y))
                if (x.function, x.index) == (y.function, y.index) =>
            {
                (0..x.lifetimes).all(|_| slot(t, v, binding, eqs))
                    && x.args
                        .iter()
                        .zip(y.args)
                        .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, binding, eqs))
            }
            (TyKind::Alias(a), TyKind::Alias(b))
                if flux_ty::regions::has_projection_inputs(self.items, a)
                    && a.trait_ == b.trait_
                    && a.name == b.name
                    && a.owner_lifetimes == b.owner_lifetimes
                    && a.family_lifetimes == b.family_lifetimes
                    && a.args.len() == b.args.len()
                    && a.family_args.len() == b.family_args.len() =>
            {
                // Abstract projections retain their occurrence inputs. Bind
                // those inputs in the same order as instantiate_family.
                (0..a.owner_lifetimes).all(|_| slot(t, v, binding, eqs))
                    && a.args
                        .iter()
                        .zip(b.args)
                        .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, binding, eqs))
                    && (0..a.family_lifetimes).all(|_| slot(t, v, binding, eqs))
                    && a.family_args
                        .iter()
                        .zip(b.family_args)
                        .all(|(&a, &b)| self.match_at(a, table, t, b, vars, v, binding, eqs))
            }
            (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
                // Expand both contracts. Signature-bound regions use local
                // sentinels and can only match the same binder position;
                // external slots still bind the impl's real region parameters.
                // Walking the signature also binds generic captured types,
                // whose region slots are supplied by the actual input types.
                let an = self.slots(pattern) as usize;
                let bn = self.slots(actual) as usize;
                let (Some(at), Some(bv)) = (table.get(*t..*t + an), vars.get(*v..*v + bn)) else {
                    return false;
                };
                let rows_a = match a.regions {
                    Some(r) => {
                        let bound: Vec<_> = (0..r.bound).map(Region::Late).collect();
                        flux_ty::regions::instantiate_fn_pointer(
                            self.items,
                            a,
                            at,
                            &bound,
                            Region::Static,
                        )
                    }
                    None if a.inputs_output.iter().all(|&ty| self.slots(ty) == 0) => {
                        Some(vec![Vec::new(); a.inputs_output.len()])
                    }
                    None => None,
                };
                let rows_b = match b.regions {
                    Some(r) => {
                        let bound: Vec<_> = (0..r.bound).map(|i| Var::MAX - i).collect();
                        flux_ty::regions::instantiate_fn_pointer(self.items, b, bv, &bound, STATIC)
                    }
                    None if b.inputs_output.iter().all(|&ty| self.slots(ty) == 0) => {
                        Some(vec![Vec::new(); b.inputs_output.len()])
                    }
                    None => None,
                };
                let (Some(rows_a), Some(rows_b)) = (rows_a, rows_b) else {
                    return false;
                };
                if rows_a.len() != rows_b.len() {
                    return false;
                }
                *t += an;
                *v += bn;
                for (((&a, &b), own), actual) in a
                    .inputs_output
                    .iter()
                    .zip(b.inputs_output)
                    .zip(&rows_a)
                    .zip(&rows_b)
                {
                    let (mut t, mut v) = (0, 0);
                    if !self.match_at(a, own, &mut t, b, actual, &mut v, binding, eqs)
                        || t != own.len()
                        || v != actual.len()
                    {
                        return false;
                    }
                }
                true
            }
            (TyKind::Dynamic(a), TyKind::Dynamic(b))
                if a.trait_ == b.trait_ && a.assoc_names == b.assoc_names =>
            {
                (0..flux_ty::regions::object_own_slots(self.items, a))
                    .all(|_| slot(t, v, binding, eqs))
                    && flux_ty::regions::object_region_types(self.items, a)
                        .zip(flux_ty::regions::object_region_types(self.items, b))
                        .all(|(a, b)| self.match_at(a, table, t, b, vars, v, binding, eqs))
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

    /// Relate values whose types can differ in nested pointer binders. Bound
    /// regions are part of a callable contract, even when neither value has
    /// any free region slots. Walk both types before using the erased slot walk.
    #[allow(clippy::too_many_arguments)]
    fn relate_types(
        &mut self,
        source: Ty,
        sub: &[Var],
        target: Ty,
        sup: &[Var],
        variance: Variance,
        category: Category,
        range: TextRange,
    ) {
        // Resolve nested projections before counting child slots. The rows
        // already describe the normalized types, in structural order.
        let source = self.normalize(source);
        let target = self.normalize(target);
        if source == target {
            return self.relate(target, sub, sup, variance, category, range);
        }
        if variance == Variance::Bivariant {
            return;
        }
        let mut children = Vec::new();
        let mut roots = 0;
        match (source.kind(), target.kind()) {
            (TyKind::FnPtr(a), TyKind::FnPtr(b)) => {
                if matches!(variance, Variance::Covariant | Variance::Invariant) {
                    self.pointer_subtype(a, sub, b, sup, range);
                }
                if matches!(variance, Variance::Contravariant | Variance::Invariant) {
                    self.pointer_subtype(b, sup, a, sub, range);
                }
                return;
            }
            (TyKind::Ref(a, x), TyKind::Ref(b, y)) if a == b => {
                roots = 1;
                children.push((
                    x,
                    y,
                    if a == Mutability::Mut {
                        Variance::Invariant
                    } else {
                        Variance::Covariant
                    },
                ));
            }
            (TyKind::RawPtr(a, x), TyKind::RawPtr(b, y)) if a == b => {
                children.push((
                    x,
                    y,
                    if a == Mutability::Mut {
                        Variance::Invariant
                    } else {
                        Variance::Covariant
                    },
                ));
            }
            (TyKind::Array(x, _), TyKind::Array(y, _))
            | (TyKind::Array(x, _), TyKind::Slice(y))
            | (TyKind::Slice(x), TyKind::Slice(y)) => children.push((x, y, Variance::Covariant)),
            (TyKind::Tuple(a), TyKind::Tuple(b)) if a.len() == b.len() => {
                children.extend(a.iter().zip(b).map(|(&a, &b)| (a, b, Variance::Covariant)));
            }
            (TyKind::Adt(a, x), TyKind::Adt(b, y)) if a == b && x.len() == y.len() => {
                roots = self.items.structs[a.strukt].generics.lifetimes.len();
                let def = self.sigs.regions.structs.get(a.strukt);
                for (i, (&x, &y)) in x.iter().zip(y).enumerate() {
                    let v = def
                        .and_then(|d| d.variances.get(roots + i))
                        .copied()
                        .unwrap_or(Variance::Invariant);
                    children.push((x, y, v));
                }
            }
            _ => {
                // Other erased-type differences retain their existing slot
                // relation; callable differences must reach a structural arm.
                return self.relate(target, sub, sup, variance, category, range);
            }
        }
        let (Some(source_row), Some(target_row)) = (
            RegionRow::new(self.items, source, sub),
            RegionRow::new(self.items, target, sup),
        ) else {
            self.errors.push(Diagnostic::new(
                range,
                "nested function pointer region metadata is not supported yet",
            ));
            return;
        };
        let root_variance = |i| match target_row.ty.kind() {
            TyKind::Adt(a, _) => self
                .sigs
                .regions
                .structs
                .get(a.strukt)
                .and_then(|d| d.variances.get(i))
                .copied()
                .unwrap_or(Variance::Invariant),
            _ => Variance::Covariant,
        };
        let roots_variance: Vec<_> = (0..roots).map(root_variance).collect();
        let mut from = source_row.cursor();
        let mut to = target_row.cursor();
        let (Some(source_roots), Some(target_roots)) = (from.roots(roots), to.roots(roots)) else {
            self.errors.push(Diagnostic::new(
                range,
                "region root metadata is not supported yet",
            ));
            return;
        };
        for ((&a, &b), v) in source_roots.iter().zip(target_roots).zip(roots_variance) {
            match variance.xform(v) {
                Variance::Covariant => self.outlives(a, b, category, range),
                Variance::Contravariant => self.outlives(b, a, category, range),
                Variance::Invariant => self.equate(a, b, category, range),
                Variance::Bivariant => {}
            }
        }
        for (x, y, v) in children {
            let (Some(source), Some(target)) = (from.child(self.items, x), to.child(self.items, y))
            else {
                self.errors.push(Diagnostic::new(
                    range,
                    "nested function pointer region metadata is not supported yet",
                ));
                return;
            };
            self.relate_types(
                source.ty,
                source.vars,
                target.ty,
                target.vars,
                variance.xform(v),
                category,
                range,
            );
        }
        debug_assert!(
            from.is_empty() && to.is_empty(),
            "structural relation consumes complete region rows"
        );
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
            TyKind::FnPtr(sig) => {
                if let Some(r) = sig.regions {
                    let (free, types) =
                        flux_ty::regions::fn_pointer_variances(self.items, self.sigs, sig)
                            .unwrap_or_else(|| {
                                (
                                    vec![Variance::Invariant; r.free as usize],
                                    vec![Variance::Invariant; r.types.len()],
                                )
                            });
                    for v in free {
                        slot(self, next, variance.xform(v));
                    }
                    for (&ty, v) in r.types.iter().zip(types) {
                        self.relate_at(ty, sub, sup, next, variance.xform(v), category, range);
                    }
                }
            }
            TyKind::Alias(alias) if flux_ty::regions::has_projection_inputs(self.items, alias) => {
                let invariant = variance.xform(Variance::Invariant);
                for _ in 0..alias.owner_lifetimes {
                    slot(self, next, invariant);
                }
                for &ty in alias.args {
                    self.relate_at(ty, sub, sup, next, invariant, category, range);
                }
                for _ in 0..alias.family_lifetimes {
                    slot(self, next, invariant);
                }
                for &ty in alias.family_args {
                    self.relate_at(ty, sub, sup, next, invariant, category, range);
                }
            }
            // As in rustc, invariant in what it captures.
            TyKind::Opaque(opaque) => {
                let v = variance.xform(Variance::Invariant);
                for _ in 0..opaque.lifetimes {
                    slot(self, next, v);
                }
                for &t in opaque.args {
                    self.relate_at(t, sub, sup, next, v, category, range);
                }
            }
            // Covariant in its lifetime bound.
            TyKind::Dynamic(object) => {
                slot(self, next, variance);
                for _ in 1..flux_ty::regions::object_own_slots(self.items, object) {
                    slot(self, next, variance.xform(Variance::Invariant));
                }
                for ty in flux_ty::regions::object_region_types(self.items, object) {
                    self.relate_at(
                        ty,
                        sub,
                        sup,
                        next,
                        variance.xform(Variance::Invariant),
                        category,
                        range,
                    );
                }
            }
            _ => {}
        }
    }

    /// The type and slots of `place`.
    fn place_regions(&mut self, place: &Place) -> (Ty, Slots) {
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
                // A `Box<T>`'s slots are its `T`'s.
                (ProjectionElem::BoxDeref, _) => {
                    ty = flux_solve::box_pointee(ty).unwrap_or(Ty::ERROR)
                }
                (
                    ProjectionElem::Index(_) | ProjectionElem::ConstantIndex { .. },
                    TyKind::Array(elem, _) | TyKind::Slice(elem),
                ) => ty = elem,
                // Of the same elements: the same slots.
                (ProjectionElem::Subslice { from, to, .. }, TyKind::Array(elem, _)) => {
                    ty = Ty::array_of(elem, Ty::usize_value(u64::from(to.saturating_sub(from))))
                }
                (ProjectionElem::Subslice { .. }, TyKind::Slice(_)) => {}
                (ProjectionElem::Downcast(v), _) => {
                    variant = Some(v);
                    continue;
                }
                (ProjectionElem::Field(i), TyKind::Tuple(list)) => {
                    let start: u32 = list[..i as usize].iter().map(|&t| self.slots(t)).sum();
                    let field = list[i as usize];
                    let n = self.slots(field);
                    vars = Slots::from_slice(&vars[start as usize..(start + n) as usize]);
                    ty = field;
                }
                (ProjectionElem::Field(i), TyKind::Closure(_, args)) => {
                    let upvars = flux_ty::ClosureArgs::split(args).upvar_types();
                    let start: u32 = upvars[..i as usize].iter().map(|&t| self.slots(t)).sum();
                    let field = upvars[i as usize];
                    let n = self.slots(field);
                    vars = Slots::from_slice(&vars[start as usize..(start + n) as usize]);
                    ty = field;
                }
                (ProjectionElem::Field(i), TyKind::Adt(..)) => {
                    let (field, field_vars) = self.field_regions(ty, &vars, variant, i);
                    ty = field;
                    vars = field_vars;
                }
                _ => {
                    ty = Ty::ERROR;
                    vars = Slots::new();
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
    ) -> (Ty, Slots) {
        let TyKind::Adt(adt, args) = ty.kind() else {
            return (Ty::ERROR, Slots::new());
        };
        let sigs = self.sigs;
        let Some(field_ty) = flux_solve::variant_fields(sigs, ty, variant.unwrap_or(0))
            .and_then(|f| f.get(field as usize).copied())
        else {
            return (Ty::ERROR, Slots::new());
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
            let mut arg_vars: SmallVec<[&[Var]; 4]> = SmallVec::new();
            let mut at = lifetimes;
            for &a in args {
                let n = self.slots(a) as usize;
                arg_vars.push(vars.get(at..at + n).unwrap_or(&[]));
                at += n;
            }
            let mut map = |r: Region| match r {
                Region::Static => Some(STATIC),
                Region::Param(i) => vars.get(i as usize).copied(),
                Region::Erased | Region::Late(_) => None,
            };
            let mut out = Slots::new();
            if self.inst(generic, table, &mut 0, &mut map, args, &arg_vars, &mut out)
                && out.len() == want as usize
            {
                return (field_ty, out);
            }
        }
        // A field flux cannot match: as long as all of the value.
        let fresh: Slots = (0..want).map(|_| self.fresh()).collect();
        for &f in &fresh {
            for &v in vars {
                self.equate(f, v, Category::Boring, TextRange::default());
            }
        }
        (field_ty, fresh)
    }

    fn operand_regions(&mut self, operand: &Operand) -> (Ty, Slots) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => self.place_regions(place),
            // Constants hold no references flux makes, other than to
            // statics and C strings, which are `'static`.
            Operand::Const(c) => {
                let ty = c.ty();
                let n = self.slots(ty);
                // An empty mutable promotion contains no element references.
                // Its storage is static, but lifetimes in the element type
                // must be inferred at the use (the pointee is invariant),
                // rather than being forced to static as an actual stored
                // reference would be. No other promoted value gets this rule.
                if let crate::Constant::Promoted { index, .. } = c
                    && let TyKind::Ref(Mutability::Mut, pointee) = ty.kind()
                    && let TyKind::Array(_, len) = pointee.kind()
                    && len.as_usize() == Some(0)
                    && matches!(
                        self.body.promoted.get(*index as usize),
                        Some(crate::eval::Value::Aggregate(fields)) if fields.is_empty()
                    )
                {
                    let mut regions: Slots = self.fresh_list(n).into_iter().collect();
                    regions[0] = STATIC;
                    return (ty, regions);
                }
                (ty, std::iter::repeat_n(STATIC, n as usize).collect())
            }
        }
    }

    /// The category of a constraint for a value going into `dest`. What
    /// goes into an opaque type is what its hidden type must outlive
    /// that counts, as rustc's errors have it.
    fn category_into(&self, dest: &Place) -> Category {
        if dest.local == Body::RETURN && dest.projection.is_empty() && self.hidden.is_empty() {
            Category::Return
        } else {
            Category::Assignment
        }
    }

    /// The constraints of every statement and call.
    fn generate(&mut self) {
        let body = self.body;
        let copy = flux_solve::traits::copy_trait_ref(self.items);
        for (block, data) in body.blocks.iter() {
            for statement in &data.statements {
                if let StatementKind::Assign(dest, rvalue) = &statement.kind {
                    self.assign(dest, rvalue, statement.range);
                    for op in rvalue.operands() {
                        self.copy_bound(copy, op, statement.range);
                    }
                }
            }
            let range = data.terminator.range;
            for op in data.terminator.kind.operands() {
                self.copy_bound(copy, op, range);
            }
            match &data.terminator.kind {
                TerminatorKind::Call {
                    func,
                    generic_args,
                    args,
                    dest,
                    ..
                } => self.call(
                    *func,
                    generic_args,
                    args,
                    dest,
                    range,
                    body.call_regions.get(block).map_or(&[], |r| &r[..]),
                ),
                TerminatorKind::CallIndirect {
                    callee, args, dest, ..
                } => {
                    let (ty, own) = self.operand_regions(callee);
                    if let TyKind::FnPtr(sig) = ty.kind()
                        && sig.regions.is_some()
                    {
                        let values: Vec<_> =
                            args.iter().map(|op| self.operand_regions(op)).collect();
                        let input_types: Vec<_> = values.iter().map(|(ty, _)| *ty).collect();
                        let inputs: Vec<_> = values.iter().map(|(_, vars)| vars.to_vec()).collect();
                        self.pointer_call(sig, &own, &input_types, &inputs, dest, range);
                    } else {
                        self.opaque_call(args, dest, range)
                    }
                }
                _ => {}
            }
        }
    }

    fn assign(&mut self, dest: &Place, rvalue: &Rvalue, range: TextRange) {
        let category = self.category_into(dest);
        let (dest_ty, dest_vars) = self.place_regions(dest);
        match rvalue {
            // A raw pointer holds no loan: a reference cast to one relates
            // only what they both point to, and other pointer casts
            // nothing.
            Rvalue::Cast(op, _) if dest_ty.is_pointer() => {
                let (op_ty, vars) = self.operand_regions(op);
                // Rust permits reinterpreting nested references through raw
                // pointers, but a direct trait-object pointer cast must retain
                // the object's lifetime bound, including after an earlier
                // reference-to-raw or supertrait coercion.
                if let (TyKind::RawPtr(_, source), TyKind::RawPtr(_, target)) =
                    (op_ty.kind(), dest_ty.kind())
                    && self.sigs.metadata(source) == flux_ty::Metadata::VTable
                    && self.sigs.metadata(target) == flux_ty::Metadata::VTable
                {
                    self.relate_types(
                        source,
                        &vars,
                        target,
                        &dest_vars,
                        Variance::Covariant,
                        category,
                        range,
                    );
                }
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
            Rvalue::WithAddr(op, _) | Rvalue::Use(op) | Rvalue::Cast(op, _) => {
                if let TyKind::FnPtr(target) = dest_ty.kind() {
                    if let Operand::Const(crate::Constant::Fn { function, args, .. }) = op {
                        let written = dest
                            .projection
                            .is_empty()
                            .then(|| self.body.reification_regions.get(dest.local))
                            .flatten();
                        self.pointer_from_item(*function, args, target, &dest_vars, written, range);
                        return;
                    }
                    let (source, own) = self.operand_regions(op);
                    if let TyKind::FnPtr(source) = source.kind()
                        && target.regions.is_some()
                        && source != target
                    {
                        self.pointer_subtype(source, &own, target, &dest_vars, range);
                        return;
                    }
                }
                let (source, vars) = self.operand_regions(op);
                self.relate_types(
                    source,
                    &vars,
                    dest_ty,
                    &dest_vars,
                    Variance::Covariant,
                    category,
                    range,
                );
            }
            // To a trait object, whose lifetime bound is the reference's
            // (or `'static` behind a raw pointer): what the value holds
            // must outlive it, as rustc's `T: 'a` for the unsizing.
            Rvalue::Unsize(op, to)
                if matches!(to.kind(), TyKind::Ref(_, p) | TyKind::RawPtr(_, p) if self.sigs.metadata(p) == flux_ty::Metadata::VTable)
                    || flux_solve::box_pointee(*to)
                        .or_else(|| flux_ty::shared_pointee(self.items, *to))
                        .is_some_and(|p| self.sigs.metadata(p) == flux_ty::Metadata::VTable) =>
            {
                let (op_ty, vars) = self.operand_regions(op);
                // The reference's own region relates; then the object's
                // bound is what the value holds must outlive (or, upcast,
                // what the object's bound must).
                let (pointee, inner, bound) = match (op_ty.kind(), dest_ty.kind()) {
                    (TyKind::Ref(_, pointee), TyKind::Ref(..)) => {
                        if let (Some(&from), Some(&to)) = (vars.first(), dest_vars.first()) {
                            self.outlives(from, to, category, range);
                        }
                        (
                            pointee,
                            vars.get(1..).unwrap_or(&[]),
                            dest_vars.get(1).copied(),
                        )
                    }
                    (TyKind::Ref(_, pointee), TyKind::RawPtr(..)) => (
                        pointee,
                        vars.get(1..).unwrap_or(&[]),
                        dest_vars.first().copied(),
                    ),
                    (TyKind::RawPtr(_, pointee), _) => {
                        (pointee, &vars[..], dest_vars.first().copied())
                    }
                    // A box's slots are its value's.
                    _ if let Some(pointee) = flux_solve::box_pointee(op_ty)
                        .or_else(|| flux_ty::shared_pointee(self.items, op_ty)) =>
                    {
                        (pointee, &vars[..], dest_vars.first().copied())
                    }
                    _ => (op_ty, &[][..], None),
                };
                // The object's elided callable input references are fresh at
                // every call. Capturing one into the closure's environment
                // must therefore fail while the object is constructed.
                let object = match to.kind() {
                    TyKind::Ref(_, object) | TyKind::RawPtr(_, object) => Some(object),
                    _ => flux_solve::box_pointee(*to)
                        .or_else(|| flux_ty::shared_pointee(self.items, *to)),
                };
                let (pointee, object) = match object.and_then(|object| {
                    flux_solve::transparent_object_unsize(
                        self.items,
                        &self.sigs.structs,
                        pointee,
                        object,
                    )
                }) {
                    Some((source, target)) => (source, Some(target)),
                    None => (pointee, object),
                };
                let mut callable_principal_checked = false;
                if let Some(object) = object
                    && let TyKind::Dynamic(d) = object.kind()
                    && flux_ty::fn_trait_kind(self.items, d.trait_).is_some()
                    && let Some(&tuple) = d.args.first()
                {
                    let count = self.slots(tuple);
                    let universal_count = self.universal.len();
                    let known_count = self.known_pairs.len();
                    let function_item = matches!(pointee.kind(), TyKind::FnDef(_));
                    let mut caller_source = pointee;
                    while let TyKind::Ref(_, inner) = caller_source.kind() {
                        caller_source = inner;
                    }
                    let caller_callable =
                        matches!(caller_source.kind(), TyKind::Param(_) | TyKind::Alias(_));
                    let universal_inputs = function_item || caller_callable;
                    let inputs = if let Some(signature) = d.callable_signature() {
                        let Some(recipe) = signature.regions else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "callable object is missing its region recipe",
                            ));
                            return;
                        };
                        let offset =
                            usize::from(matches!(to.kind(), TyKind::Ref(..) | TyKind::RawPtr(..)))
                                + 1;
                        let Some(own) = dest_vars.get(offset..) else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "callable object region recipe does not match its occurrence",
                            ));
                            return;
                        };
                        let bound: Vec<_> = (0..recipe.bound)
                            .map(|i| {
                                if universal_inputs {
                                    self.new_universal(format!("'callable{i}"))
                                } else {
                                    let var = self.fresh();
                                    self.holes.insert(var);
                                    var
                                }
                            })
                            .collect();
                        let Some(rows) = flux_ty::regions::instantiate_fn_pointer(
                            self.items, signature, own, &bound, STATIC,
                        ) else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "callable object region recipe does not match its occurrence",
                            ));
                            return;
                        };
                        rows[..signature.params().len()]
                            .iter()
                            .flatten()
                            .copied()
                            .collect()
                    } else if universal_inputs {
                        (0..count)
                            .map(|i| self.new_universal(format!("'callable{i}")))
                            .collect()
                    } else {
                        self.fresh_list(count)
                    };
                    if !universal_inputs && d.callable.is_none() {
                        for &input in &inputs {
                            self.holes.insert(input);
                        }
                    }
                    let mut implied = Vec::new();
                    flux_ty::regions::implied_outlives(
                        self.items,
                        self.sigs,
                        tuple,
                        &inputs,
                        &mut implied,
                    );
                    for relation in implied {
                        if let Outlives::Region(a, b) = relation {
                            if universal_inputs {
                                self.known_pairs.push((a, b));
                            } else {
                                self.hole_pairs.insert((a, b));
                            }
                        }
                    }
                    if universal_inputs {
                        self.close_known();
                    }
                    match pointee.kind() {
                        TyKind::Closure(id, list) => {
                            callable_principal_checked = true;
                            self.closure_goal(id, list, Some(inner), Some(&inputs), None, range)
                        }
                        TyKind::FnDef(def) => {
                            callable_principal_checked = true;
                            let regions = self.sigs.regions.fns.get(def.function);
                            if let Some(signature) = d.callable_signature()
                                && d.assoc_tys.iter().any(|&t| self.slots(t) != 0)
                            {
                                let offset = usize::from(matches!(
                                    to.kind(),
                                    TyKind::Ref(..) | TyKind::RawPtr(..)
                                )) + 1;
                                let own = &dest_vars[offset..];
                                self.pointer_from_item(
                                    def.function,
                                    def.args,
                                    signature,
                                    own,
                                    None,
                                    range,
                                );
                            } else if !regions.is_some_and(|r| {
                                r.owner == 0
                                    && r.callback_binder
                                        == flux_ty::regions::CallbackBinder::Independent
                            }) || def.args.iter().any(|&t| self.slots(t) != 0)
                                || d.assoc_tys.iter().any(|&t| self.slots(t) != 0)
                            {
                                self.errors.push(Diagnostic::new(range, "higher-ranked callable object function regions are not supported yet"));
                            } else {
                                let old = std::mem::replace(&mut self.call_range, range);
                                let input_types = match tuple.kind() {
                                    TyKind::Tuple(types) => types,
                                    _ => &[],
                                };
                                if self
                                    .function_output_typed(pointee, input_types, &inputs)
                                    .is_none()
                                {
                                    self.errors.push(Diagnostic::new(range, "function item callback region metadata is not supported yet"));
                                }
                                self.call_range = old;
                            }
                        }
                        TyKind::FnPtr(sig) => {
                            callable_principal_checked = true;
                            if d.assoc_tys.iter().any(|&t| self.slots(t) != 0) {
                                self.errors.push(Diagnostic::new(
                                    range,
                                    "callable object pointer output regions are not supported yet",
                                ));
                            } else if sig.regions.is_some() {
                                let TyKind::Tuple(input_types) = tuple.kind() else {
                                    return;
                                };
                                let mut at = 0;
                                let mut rows = Vec::new();
                                for &ty in input_types {
                                    let n = self.slots(ty) as usize;
                                    rows.push(inputs.get(at..at + n).unwrap_or(&[]).to_vec());
                                    at += n;
                                }
                                if self
                                    .pointer_inputs(sig, inner, input_types, &rows, range)
                                    .is_none()
                                {
                                    self.errors.push(Diagnostic::new(range, "callable object pointer region metadata is not supported yet"));
                                }
                            }
                        }
                        TyKind::Dynamic(_) => {}
                        _ if caller_callable => {
                            callable_principal_checked = true;
                            let goal = flux_ty::Predicate {
                                self_ty: pointee,
                                trait_ref: flux_ty::TraitRef {
                                    trait_: d.trait_,
                                    name: d.name,
                                    args: d.args,
                                },
                            };
                            self.prove(goal, &[inner.to_vec(), inputs.clone()], &[], range, 0);
                        }
                        _ => {}
                    }
                    if universal_inputs {
                        // These universals belong to this proof, not to the
                        // enclosing function or closure's exported contract.
                        if !self.caller_equalities_fit(&[]) {
                            self.errors.push(Diagnostic::new(
                                range,
                                "callable is not general enough for this trait object",
                            ));
                        }
                        for &(var, _) in &self.universal[universal_count..] {
                            self.universal_of.remove(&var);
                        }
                        self.universal.truncate(universal_count);
                        self.known_pairs.truncate(known_count);
                        self.close_known();
                    }
                }
                // Implementing an object's principal also requires the chosen
                // impl's lifetime contract (for example Any's T: 'static).
                // Builtin callable proofs above use fresh input-binder rows,
                // which are not physical fields in the destination object.
                // They have no chosen user impl whose lifetime contract needs
                // this ordinary principal proof (e.g. Any's T: 'static).
                if !pointee.is_dyn()
                    && !callable_principal_checked
                    && let Some(object) = object
                    && let TyKind::Dynamic(d) = object.kind()
                {
                    let mut rows = vec![inner.to_vec()];
                    let object_start = usize::from(matches!(dest_ty.kind(), TyKind::Ref(..)));
                    let owner_count = self.items.traits[d.trait_].generics.lifetimes.len();
                    let start = object_start + 1 + owner_count;
                    let mut at = start;
                    for &arg in d.args {
                        let count = self.slots(arg) as usize;
                        rows.push(dest_vars.get(at..at + count).unwrap_or(&[]).to_vec());
                        at += count;
                    }
                    // Supertrait Self: 'static constrains the erased payload,
                    // including closure environments and function-item args.
                    let static_self = flux_solve::objects::supertraits_of(self.items, d.trait_)
                        .into_iter().any(|owner| self.sigs.regions.traits.get(owner).is_some_and(|contract| {
                            contract.outlives.iter().any(|requirement| matches!(requirement,
                                Outlives::Type(ty, Region::Static) if matches!(ty.kind(), TyKind::Param(p) if p.index == 0)))
                        }));
                    if static_self {
                        for &var in inner {
                            self.outlives(var, STATIC, category, range);
                        }
                        self.type_outlives(pointee, STATIC, range);
                    }
                    self.prove(
                        flux_solve::traits::dyn_principal(pointee, d),
                        &rows,
                        dest_vars.get(object_start + 1..start).unwrap_or(&[]),
                        range,
                        0,
                    );
                }
                if let Some(object) = object
                    && let TyKind::Dynamic(d) = object.kind()
                    && flux_ty::fn_trait_kind(self.items, d.trait_).is_none()
                {
                    let offset = usize::from(matches!(dest_ty.kind(), TyKind::Ref(..)));
                    let object_vars = dest_vars.get(offset..).unwrap_or(&[]);
                    let mut rows = vec![inner.to_vec()];
                    let owner_count = self.items.traits[d.trait_].generics.lifetimes.len();
                    let mut at = 1 + owner_count;
                    for &arg in d.args {
                        let count = self.slots(arg) as usize;
                        let Some(row) = object_vars.get(at..at + count) else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "object argument region rows do not match",
                            ));
                            return;
                        };
                        rows.push(row.to_vec());
                        at += count;
                    }
                    if let TyKind::Dynamic(source) = pointee.kind() {
                        if source.trait_ != d.trait_
                            && (owner_count != 0 || d.args.iter().any(|&ty| self.slots(ty) != 0))
                        {
                            self.errors.push(Diagnostic::new(
                                range,
                                "inherited object argument regions are not supported yet",
                            ));
                        } else if source.trait_ == d.trait_ {
                            for index in 1..1 + owner_count {
                                if let (Some(&from), Some(&to)) =
                                    (inner.get(index), object_vars.get(index))
                                {
                                    self.outlives(from, to, category, range);
                                    self.outlives(to, from, category, range);
                                }
                            }
                            let mut from_at = 1 + owner_count;
                            for (i, (&from, &to)) in source.args.iter().zip(d.args).enumerate() {
                                let count = self.slots(from) as usize;
                                let Some(actual) = inner.get(from_at..from_at + count) else {
                                    self.errors.push(Diagnostic::new(
                                        range,
                                        "object argument region rows do not match",
                                    ));
                                    return;
                                };
                                self.relate_types(
                                    from,
                                    actual,
                                    to,
                                    &rows[i + 1],
                                    Variance::Invariant,
                                    category,
                                    range,
                                );
                                from_at += count;
                            }
                        }
                    }
                    for (owner, name) in flux_solve::objects::dyn_assoc(self.items, d.trait_) {
                        let Some(row) =
                            flux_ty::regions::object_associated_range(self.items, d, name)
                                .and_then(|row| object_vars.get(row))
                        else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "object equality region rows do not match",
                            ));
                            return;
                        };
                        if row.is_empty() {
                            continue;
                        }
                        // The principal selection gives the impl and Self row;
                        // equality outputs must agree independently of the
                        // object's own lifetime bound.
                        let reference = flux_solve::traits::dyn_bounds(self.sigs, pointee, d)
                            .into_iter()
                            .find(|bound| bound.trait_ref.trait_ == owner)
                            .map(|bound| bound.trait_ref);
                        let Some(reference) = reference else {
                            continue;
                        };
                        if owner != d.trait_ && reference.args.iter().any(|&ty| self.slots(ty) != 0)
                        {
                            self.errors.push(Diagnostic::new(
                                range,
                                "inherited object argument regions are not supported yet",
                            ));
                            continue;
                        }
                        let alias =
                            flux_solve::traits::alias_of(self.items, reference, pointee, name);
                        let alias_rows = if owner == d.trait_ {
                            rows.clone()
                        } else {
                            std::iter::once(rows[0].clone())
                                .chain(reference.args.iter().map(|_| Vec::new()))
                                .collect()
                        };
                        let Some(actual) = self.normalize_regions(alias, &alias_rows, 0) else {
                            self.errors.push(Diagnostic::new(
                                range,
                                "object equality input regions are not supported yet",
                            ));
                            continue;
                        };
                        if actual.len() != row.len() {
                            self.errors.push(Diagnostic::new(
                                range,
                                "object equality region rows do not match",
                            ));
                            continue;
                        }
                        for (a, &b) in actual.into_iter().zip(row) {
                            self.equate(a, b, category, range);
                        }
                    }
                }
                if let Some(bound) = bound {
                    // An existing object's argument rows are contracts, not
                    // its erased stored payload. Only its bound is shortened.
                    let stored = if pointee.is_dyn() {
                        inner.get(..1).unwrap_or(&[])
                    } else {
                        inner
                    };
                    if pointee.is_dyn() {
                        for &v in stored {
                            self.outlives(v, bound, category, range);
                        }
                    } else {
                        self.stored_type_outlives(pointee, stored, bound, category, range);
                    }
                }
            }
            Rvalue::Unsize(op, _) | Rvalue::Subslice { slice: op, .. } => {
                let (source, vars) = self.operand_regions(op);
                self.relate_types(
                    source,
                    &vars,
                    dest_ty,
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
                raw: true,
                mutability,
                place,
                ..
            } => {
                let (place_ty, place_vars) = self.place_regions(place);
                let variance = if *mutability == Mutability::Mut {
                    Variance::Invariant
                } else {
                    Variance::Covariant
                };
                self.relate(place_ty, &place_vars, &dest_vars, variance, category, range);
            }
            Rvalue::Ref {
                mutability, place, ..
            } => {
                let loan = self.fresh();
                self.loan_vars.push(loan);
                let (place_ty, place_vars) = self.place_regions(place);
                if let Some((&first, rest)) = dest_vars.split_first() {
                    self.outlives(loan, first, category, range);
                    // A trait object reborrowed mutably may shorten its
                    // lifetime bound, as rustc's unsizing coercion does
                    // (no other object can be written in its place).
                    let variance = match mutability {
                        Mutability::Mut if !place_ty.is_dyn() => Variance::Invariant,
                        _ => Variance::Covariant,
                    };
                    self.relate_types(
                        place_ty,
                        &place_vars,
                        match dest_ty.kind() {
                            TyKind::Ref(_, inner) => inner,
                            _ => place_ty,
                        },
                        rest,
                        variance,
                        category,
                        range,
                    );
                }
                self.reborrow(place, loan, range);
            }
            Rvalue::InlineAsm { inputs, .. } => {
                // Unsafe inout operands retain their declared type/region rows;
                // the machine value itself is opaque to the compiler.
                self.aggregate(
                    AggregateKind::Tuple,
                    inputs,
                    dest_ty,
                    &dest_vars,
                    category,
                    range,
                )
            }
            Rvalue::Aggregate(kind, ops) => {
                self.aggregate(*kind, ops, dest_ty, &dest_vars, category, range)
            }
            Rvalue::OffsetOf(..)
            | Rvalue::BinaryOp(..)
            | Rvalue::UnaryOp(..)
            | Rvalue::Discriminant(_)
            | Rvalue::DiscriminantForDrop(_)
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
                projection: place.projection.prefix(k),
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
        self.adt_obligations(dest_ty, dest_vars, range);
        match kind {
            AggregateKind::Union(_, index) => {
                if let Some(op) = ops.first() {
                    let (field_ty, field) = self.field_regions(dest_ty, dest_vars, None, index);
                    let (source, vars) = self.operand_regions(op);
                    self.relate_types(
                        source,
                        &vars,
                        field_ty,
                        &field,
                        Variance::Covariant,
                        category,
                        range,
                    );
                }
            }
            AggregateKind::Tuple => {
                let TyKind::Tuple(list) = dest_ty.kind() else {
                    return;
                };
                let mut at = 0;
                for (op, &t) in ops.iter().zip(list) {
                    let n = self.slots(t) as usize;
                    let (source, vars) = self.operand_regions(op);
                    let field = dest_vars.get(at..at + n).unwrap_or(&[]).to_vec();
                    self.relate_types(
                        source,
                        &vars,
                        t,
                        &field,
                        Variance::Covariant,
                        category,
                        range,
                    );
                    at += n;
                }
            }
            AggregateKind::Array(elem) => {
                for op in ops {
                    let (source, vars) = self.operand_regions(op);
                    self.relate_types(
                        source,
                        &vars,
                        elem,
                        dest_vars,
                        Variance::Covariant,
                        category,
                        range,
                    );
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
                    let (source, vars) = self.operand_regions(op);
                    self.relate_types(
                        source,
                        &vars,
                        field_ty,
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
                    let (source, vars) = self.operand_regions(op);
                    let field = dest_vars.get(at..at + n).unwrap_or(&[]).to_vec();
                    self.relate_types(
                        source,
                        &vars,
                        t,
                        &field,
                        Variance::Covariant,
                        category,
                        range,
                    );
                    at += n;
                }
                // What its body requires of what it captures.
                match self.requirements.get(&id) {
                    Some(required) => {
                        self.closure_type_requirements(
                            required,
                            args,
                            true,
                            |e| match e {
                                Ext::Static => Some(STATIC),
                                Ext::Parent(_, k) => Some(1 + k),
                                Ext::Upvar(k) => dest_vars.get(k as usize).copied(),
                                _ => None,
                            },
                            range,
                        );
                        for &(a, b, _) in &required.outlives {
                            let map = |e: Ext| match e {
                                Ext::Static => Some(STATIC),
                                Ext::Parent(_, k) => Some(1 + k),
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

    #[allow(clippy::too_many_arguments)]
    fn call(
        &mut self,
        func: flux_hir::FunctionId,
        generic_args: &[Ty],
        args: &[Operand],
        dest: &Place,
        range: TextRange,
        written: &[(u32, Box<[Region]>)],
    ) {
        let (items, sigs) = (self.items, self.sigs);
        if crate::fn_method_kind(items, func).is_some()
            && let Some(&self_ty) = generic_args.first()
            && let Some((sig, object)) = match self_ty.kind() {
                TyKind::FnPtr(sig) if sig.regions.is_some() => Some((sig, false)),
                TyKind::Dynamic(object) => object.callable_signature().map(|sig| (sig, true)),
                _ => None,
            }
        {
            let Some(receiver) = args.first() else {
                return;
            };
            let (mut ty, mut own) = self.operand_regions(receiver);
            while let TyKind::Ref(_, inner) = ty.kind() {
                if own.is_empty() {
                    break;
                }
                own.remove(0);
                ty = inner;
            }
            if object {
                // The payload lifetime belongs to the object, not its callable
                // signature's free region captures.
                if own.is_empty() {
                    return;
                }
                own.remove(0);
            }
            let (tuple, flat) = args
                .get(1)
                .map(|arg| self.operand_regions(arg))
                .unwrap_or((Ty::UNIT, Slots::new()));
            let TyKind::Tuple(input_types) = tuple.kind() else {
                return;
            };
            let mut flat = flat.to_vec();
            let mut inputs = Vec::new();
            for &ty in input_types {
                let n = self.slots(ty) as usize;
                if n > flat.len() {
                    self.errors.push(Diagnostic::new(
                        range,
                        "function pointer input region metadata is not supported yet",
                    ));
                    return;
                }
                inputs.push(flat.drain(..n).collect());
            }
            return self.pointer_call(sig, &own, input_types, &inputs, dest, range);
        }
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
        // Closures passed on, which the callee may call: its bounds
        // say how, as they are proven below.
        let mut passed = Vec::new();
        self.call_inputs.clear();
        for op in args {
            let (ty, vars) = self.operand_regions(op);
            self.closures_in(ty, &vars, &mut 0, &mut passed);
            self.call_inputs.extend(vars);
        }
        self.proven_closures.clear();
        self.output_closures.clear();
        self.call_range = range;
        let Some(regions) = sigs.regions.fns.get(func) else {
            self.missing_trait_owner_regions(func, range);
            for (id, closure_args, upvars) in passed {
                self.passed_closure(id, closure_args, &upvars, args, dest, range, false);
            }
            return self.opaque_call(args, dest, range);
        };
        let sig = &sigs.fns[func];
        let Ok(selected) = self.selected_source_signature(func, range) else {
            return;
        };
        let lifetimes = self.fresh_list(regions.names.len() as u32);
        let type_vars = self.instantiated_type_vars(generic_args);
        // The type argument is fixed by the caller's spelling, even where
        // the callee's parameters/results alone would allow a shorter region.
        for (index, table) in written {
            let Some(vars) = type_vars.get(*index as usize) else {
                continue;
            };
            for (&var, region) in vars.iter().zip(table.iter()) {
                let declared = match *region {
                    Region::Static => Some(STATIC),
                    Region::Param(i) => Some(1 + i),
                    Region::Erased | Region::Late(_) => None,
                };
                if let Some(declared) = declared {
                    self.equate(var, declared, Category::CallArgument, range);
                }
            }
            let mut implied = Vec::new();
            flux_ty::regions::implied_outlives_with_static(
                items,
                sigs,
                generic_args[*index as usize],
                vars,
                Some(STATIC),
                &mut implied,
            );
            for constraint in implied {
                match constraint {
                    Outlives::Region(a, b) => self.outlives(a, b, Category::CallArgument, range),
                    Outlives::Type(t, b) => self.type_outlives(t, b, range),
                }
            }
        }
        let mut map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        if let Some((_, table)) = written
            .iter()
            .find(|(index, _)| *index == flux_ty::regions::IMPL_SELF_ARG)
        {
            self.impl_self_annotation(func, table, &mut map, generic_args, &type_vars, range);
        }
        // `<T as Tr<'a>>::f`: the trait's lifetimes, the first of `func`'s.
        if let Some((_, table)) = written
            .iter()
            .find(|(index, _)| *index == flux_ty::regions::TRAIT_LIFETIMES_ARG)
        {
            if regions.owner as usize != table.len() {
                self.errors.push(Diagnostic::new(
                    range,
                    "lifetime arguments of this trait are not supported yet",
                ));
            } else {
                for (&var, region) in lifetimes.iter().zip(table.iter()) {
                    let declared = match *region {
                        Region::Static => Some(STATIC),
                        Region::Param(i) => Some(1 + i),
                        Region::Erased | Region::Late(_) => None,
                    };
                    if let Some(declared) = declared {
                        self.equate(var, declared, Category::CallArgument, range);
                    }
                }
            }
        }
        let mut inputs: Vec<Var> = Vec::new();
        let mut unmatched_inputs: Vec<Var> = Vec::new();
        for (i, op) in args.iter().enumerate() {
            let (ty, actual) = self.operand_regions(op);
            if selected.is_some() && actual.len() != self.slots(self.normalize(ty)) as usize {
                self.errors.push(Diagnostic::new(
                    range,
                    "selected method argument region rows do not match",
                ));
                return;
            }
            inputs.extend(&actual);
            let (Some(&formal_ty), Some(table)) = (sig.params.get(i), regions.params.get(i)) else {
                continue;
            };
            let (formal_ty, table) = selected.as_ref().map_or((formal_ty, &**table), |selected| {
                (selected.params[i].ty, &*selected.params[i].regions)
            });
            let formal = self.instantiate_sig(formal_ty, table, &mut map, generic_args, &type_vars);
            let formal = if selected.is_some() {
                self.selected_value_shape(
                    formal_ty.subst(generic_args),
                    formal,
                    sig.params[i].subst(generic_args),
                    range,
                )
            } else {
                formal
            };
            if selected.is_some() && formal.is_none() {
                return;
            }
            match formal {
                Some(formal) => self.relate_types(
                    ty,
                    &actual,
                    formal_ty.subst(generic_args),
                    &formal,
                    Variance::Covariant,
                    Category::CallArgument,
                    range,
                ),
                None => unmatched_inputs.extend(&actual),
            }
        }
        let category = self.category_into(dest);
        let (dest_ty, dest_vars) = self.place_regions(dest);
        if selected.is_some() && dest_vars.len() != self.slots(self.normalize(dest_ty)) as usize {
            self.errors.push(Diagnostic::new(
                range,
                "selected method result region rows do not match",
            ));
            return;
        }
        let (ret, table) = selected
            .as_ref()
            .map_or((sig.ret, &*regions.ret), |selected| {
                (selected.ret.ty, &*selected.ret.regions)
            });
        let formal = self.instantiate_sig(ret, table, &mut map, generic_args, &type_vars);
        let formal = if selected.is_some() {
            self.selected_value_shape(
                ret.subst(generic_args),
                formal,
                sig.ret.subst(generic_args),
                range,
            )
        } else {
            formal
        };
        if selected.is_some() && formal.is_none() {
            return;
        }
        match formal {
            Some(formal) => self.relate_types(
                ret.subst(generic_args),
                &formal,
                dest_ty,
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
        self.signature_obligations(func, &lifetimes, generic_args, &type_vars, range);
        for (id, closure_args, upvars) in passed {
            self.passed_closure(id, closure_args, &upvars, args, dest, range, true);
        }
    }

    /// `S::<'a>::f`: the self type of `func`'s `impl` block, instantiated
    /// for the call, is the written `table`'s, as rustc's user type
    /// annotation has it.
    fn impl_self_annotation(
        &mut self,
        func: flux_hir::FunctionId,
        table: &[Region],
        map: &mut dyn FnMut(Region) -> Option<Var>,
        generic_args: &[Ty],
        type_vars: &[Vec<Var>],
        range: TextRange,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        let vars = items.impl_of(func).and_then(|imp| {
            let self_ty = *sigs.impls.get(imp)?;
            let impl_regions = sigs.regions.impls.get(imp)?;
            // The block's lifetimes are the first of the function's.
            if sigs.regions.fns.get(func)?.owner as usize != impl_regions.names.len() {
                return None;
            }
            let own = &impl_regions.self_ty;
            let vars = self.instantiate_sig(self_ty, own, map, generic_args, type_vars)?;
            (vars.len() == table.len()).then(|| (self_ty.subst(generic_args), vars))
        });
        let Some((self_ty, vars)) = vars else {
            self.errors.push(Diagnostic::new(
                range,
                "lifetime arguments of this `impl` block's self type are not supported yet",
            ));
            return;
        };
        for (&var, region) in vars.iter().zip(table) {
            let declared = match *region {
                Region::Static => Some(STATIC),
                Region::Param(i) => Some(1 + i),
                Region::Erased | Region::Late(_) => None,
            };
            if let Some(declared) = declared {
                self.equate(var, declared, Category::CallArgument, range);
            }
        }
        let mut implied = Vec::new();
        flux_ty::regions::implied_outlives_with_static(
            items,
            sigs,
            self_ty,
            &vars,
            Some(STATIC),
            &mut implied,
        );
        for constraint in implied {
            match constraint {
                Outlives::Region(a, b) => self.outlives(a, b, Category::CallArgument, range),
                Outlives::Type(t, b) => self.type_outlives(t, b, range),
            }
        }
    }

    fn missing_trait_owner_regions(&mut self, func: flux_hir::FunctionId, range: TextRange) {
        if matches!(
            self.items.functions[func].owner,
            Some(flux_hir::Owner::Trait(_))
        ) {
            self.errors.push(Diagnostic::new(
                range,
                "trait method owner region metadata is not supported yet",
            ));
        }
    }

    /// Enforces the complete signature contract for both direct calls and
    /// function items used through callable bounds. Type selection erases
    /// regions, so owner and function predicates must also constrain the
    /// instantiated argument slots before their output can escape.
    fn signature_obligations(
        &mut self,
        func: flux_hir::FunctionId,
        lifetimes: &[Var],
        generic_args: &[Ty],
        type_vars: &[Vec<Var>],
        range: TextRange,
    ) {
        self.signature_obligations_for(
            func,
            lifetimes,
            generic_args,
            type_vars,
            range,
            FunctionUse::Call,
        );
    }

    fn signature_obligations_for(
        &mut self,
        func: flux_hir::FunctionId,
        lifetimes: &[Var],
        generic_args: &[Ty],
        type_vars: &[Vec<Var>],
        range: TextRange,
        use_: FunctionUse,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        let Some(regions) = sigs.regions.fns.get(func) else {
            self.missing_trait_owner_regions(func, range);
            return;
        };
        let map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        // Even zero-sized operations can form a bounded ADT as a written type
        // argument without ever materializing a value of that type.
        for (&ty, vars) in generic_args.iter().zip(type_vars) {
            self.adt_obligations(ty, vars, range);
        }
        // The callee's bounds, and what its types imply.
        for p in regions.outlives.iter() {
            match *p {
                Outlives::Region(a, b) => {
                    if use_ == FunctionUse::Reification && !regions.has_bounds {
                        continue;
                    }
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        self.outlives(a, b, Category::CallArgument, range);
                    }
                }
                Outlives::Type(t, b) => {
                    let Some(b) = map(b) else { continue };
                    let instance = self.normalize(t.subst(generic_args));
                    if let TyKind::Param(p) = t.kind()
                        && let Some(vars) = type_vars.get(p.index as usize)
                    {
                        if matches!(instance.kind(), TyKind::Alias(alias)
                            if flux_ty::regions::has_projection_inputs(items, alias))
                        {
                            let env = self.caller_region_goals();
                            if self.constrain_family_outlives(instance, vars, b, &env, range) {
                                continue;
                            }
                        }
                        for &v in &vars.clone() {
                            self.outlives(v, b, Category::CallArgument, range);
                        }
                    }
                    self.type_outlives(instance, b, range);
                }
            }
        }
        // Its trait bounds (`Self: Tr` first, for a trait's method), as
        // the `impl` blocks for the call's types meet them, and what they
        // say associated types are.
        let owner = items.functions[func].owner;
        let block = match owner {
            Some(flux_hir::Owner::Impl(imp)) => sigs.regions.impls.get(imp).map(|r| (imp, r)),
            _ => None,
        };
        let owner_schema = match owner {
            Some(flux_hir::Owner::Trait(trait_)) => {
                flux_ty::regions::trait_bound_regions(items, sigs, trait_)
            }
            _ => None,
        };
        let mut owner_declared = Vec::new();
        let mut bounds: Vec<(flux_ty::Predicate, &[Region], TraitRefRegions)> = Vec::new();
        match (owner, block) {
            (Some(flux_hir::Owner::Trait(t)), _) => {
                let list = flux_solve::traits::owner_predicates(items, sigs, owner);
                let expected = flux_solve::traits::trait_self_predicate(items, t);
                let metadata = sigs.regions.traits.get(t);
                let count = expected.trait_ref.args.len();
                // This prefix belongs to the trait declaration, not to the
                // selected impl's binder. Param tables expand through the
                // very same type_vars as the inputs and output of this call.
                let valid = metadata.is_some_and(|r| {
                    r.names.len() == items.traits[t].generics.lifetimes.len()
                        && r.names.len() == regions.owner as usize
                        && regions.names.get(..regions.owner as usize) == Some(&r.names[..])
                }) && regions.owner as usize <= regions.names.len()
                    && lifetimes.len() == regions.names.len()
                    && list.first() == Some(&expected)
                    && generic_args.len() > count
                    && type_vars.len() == generic_args.len()
                    && generic_args
                        .iter()
                        .zip(type_vars)
                        .take(count + 1)
                        .all(|(&t, v)| {
                            let value = self.normalize(t);
                            v.len() == self.slots(value) as usize
                        });
                if !valid {
                    self.errors.push(Diagnostic::new(
                        range,
                        "trait method owner region metadata is not supported yet",
                    ));
                } else {
                    bounds.push((
                        expected,
                        &[],
                        TraitRefRegions {
                            bound: 0,
                            lifetimes: (0..regions.owner).map(Region::Param).collect(),
                            // One table per declared type argument, even when
                            // its substituted type contributes multiple slots.
                            args: vec![Box::default(); count].into(),
                        },
                    ));
                }
                if let Some((selfs, traits, equalities)) = &owner_schema {
                    if selfs.len() != list.len() - 1
                        || traits.len() != list.len() - 1
                        || equalities.len() != sigs.traits[t].header_projections.len()
                    {
                        self.errors.push(Diagnostic::new(
                            range,
                            "trait method owner region tables do not match",
                        ));
                    } else {
                        for ((&predicate, row), contract) in
                            list.iter().skip(1).zip(selfs).zip(traits)
                        {
                            bounds.push((predicate, row, contract.clone()));
                        }
                        owner_declared.extend(
                            sigs.traits[t]
                                .header_projections
                                .iter()
                                .copied()
                                .zip(equalities.iter().cloned()),
                        );
                    }
                } else if list.len() > 1 || !sigs.traits[t].header_projections.is_empty() {
                    self.errors.push(Diagnostic::new(
                        range,
                        "trait method owner region tables cannot be resolved",
                    ));
                }
            }
            (Some(flux_hir::Owner::Impl(_)), Some((imp, r))) => {
                let list = sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
                for (k, &p) in list.iter().enumerate() {
                    bounds.push((
                        p,
                        r.bound_selfs.get(k).map_or(&[][..], |t| t),
                        r.bound_traits.get(k).cloned().unwrap_or_default(),
                    ));
                }
            }
            _ => {
                for p in flux_solve::traits::owner_predicates(items, sigs, owner) {
                    bounds.push((p, &[], TraitRefRegions::default()));
                }
            }
        }
        let own = sigs.fn_predicates.get(func).map_or(&[][..], |p| p);
        for (k, &p) in own.iter().enumerate() {
            bounds.push((
                p,
                regions.bound_selfs.get(k).map_or(&[][..], |t| t),
                regions.bound_traits.get(k).cloned().unwrap_or_default(),
            ));
        }
        let mut declared: Vec<(flux_ty::traits::Projection, &[Region])> = Vec::new();
        declared.extend(
            owner_declared
                .iter()
                .map(|(projection, row)| (*projection, row.as_ref())),
        );
        if let Some((imp, r)) = block {
            let list = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
            declared.extend(
                list.iter()
                    .copied()
                    .zip(r.projections.iter().map(|t| &t[..])),
            );
        }
        let list = sigs.fn_projections.get(func).map_or(&[][..], |p| p);
        declared.extend(
            list.iter()
                .copied()
                .zip(regions.projections.iter().map(|t| &t[..])),
        );
        let own_projections = sigs
            .fn_projections
            .get(func)
            .map_or(&[][..], |projections| projections);
        if !self.callable_outputs_complete(
            &items.functions[func].generics,
            own,
            own_projections,
            range,
        ) {
            return;
        }
        if let Some((owner, _)) = block {
            let predicates = sigs
                .impl_predicates
                .get(owner)
                .map_or(&[][..], |predicates| predicates);
            let projections = sigs
                .impl_projections
                .get(owner)
                .map_or(&[][..], |projections| projections);
            if !self.callable_outputs_complete(
                &items.impls[owner].generics,
                predicates,
                projections,
                range,
            ) {
                return;
            }
        }
        let previous = std::mem::replace(
            &mut self.call_projections,
            declared.iter().map(|&(p, t)| (p, t.into())).collect(),
        );
        if !self.callable_bound_inputs_unambiguous(func, &bounds, range) {
            self.call_projections = previous;
            return;
        }
        for (p, table, trait_regions) in &bounds {
            self.prove_bound(
                *p,
                table,
                trait_regions,
                lifetimes,
                generic_args,
                type_vars,
                range,
                0,
            );
        }
        self.call_projections = previous;
        let mut inputs = vec![None; owner_declared.len()];
        if let Some((imp, _)) = block {
            let projections = sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
            for index in 0..projections.len() {
                inputs.push(self.declared_projection_input(
                    &items.impls[imp].generics,
                    projections,
                    index,
                    &bounds,
                    range,
                ));
            }
        }
        let function_bounds = &bounds[bounds.len() - own.len()..];
        for index in 0..own_projections.len() {
            inputs.push(self.declared_projection_input(
                &items.functions[func].generics,
                own_projections,
                index,
                function_bounds,
                range,
            ));
        }
        for ((p, table), input) in declared.into_iter().zip(inputs) {
            self.prove_projection(
                p,
                table,
                input.as_ref(),
                lifetimes,
                generic_args,
                type_vars,
                range,
            );
        }
    }

    /// A copy of a struct or enum, or of an array of them, needs its type
    /// to be `Copy` for its lifetimes too (`impl Copy for X<'static>`),
    /// as rustc proves at each copy.
    fn copy_bound(&mut self, copy: Option<flux_ty::TraitRef>, op: &Operand, range: TextRange) {
        let (Some(copy), Operand::Copy(place)) = (copy, op) else {
            return;
        };
        // rustc moves temporaries rather than copying them.
        if place.projection.is_empty() && self.body.locals[place.local].kind.is_temp() {
            return;
        }
        let mut elem = self.body.place_ty(place, self.sigs);
        while let TyKind::Array(t, _) = elem.kind() {
            elem = t;
        }
        if !matches!(elem.kind(), TyKind::Adt(..)) || self.slots(elem) == 0 {
            return;
        }
        let (ty, vars) = self.place_regions(place);
        self.proving = Category::Boring;
        self.prove(copy.with_self(ty), &[vars.into_vec()], &[], range, 0);
        self.proving = Category::CallArgument;
    }

    /// The slots of an instance of `ty`, a type in a bound's terms (whose
    /// own lifetimes are not written down, so fresh), for generic
    /// arguments `args` with slots `arg_vars`; `None` where they cannot
    /// be matched to the instance's type.
    fn bound_slots(&mut self, ty: Ty, args: &[Ty], arg_vars: &[Vec<Var>]) -> Option<Vec<Var>> {
        self.bound_slots_in(ty, &[], &[], args, arg_vars)
    }

    /// Erased-equal callable predicates cannot borrow each other's input
    /// occurrence rows. Identical duplicates describe the same contract.
    fn callable_bound_inputs_unambiguous(
        &mut self,
        function: flux_hir::FunctionId,
        bounds: &[(flux_ty::Predicate, &[Region], TraitRefRegions)],
        range: TextRange,
    ) -> bool {
        // These keys live in the declaration's namespace. A caller's Cx.env
        // or concrete substitutions could conflate independent P/Q binders.
        let facts = flux_solve::traits::caller_projections_of(self.items, self.sigs, function);
        let keys: Vec<_> = bounds
            .iter()
            .map(|(predicate, _, _)| {
                if predicate.self_ty.has_infer() {
                    return None;
                }
                let key =
                    flux_solve::traits::normalize(self.items, self.sigs, &facts, predicate.self_ty);
                (!key.has_infer() && !key.has_error()).then_some(key)
            })
            .collect();
        for (index, (predicate, self_row, rows)) in bounds.iter().enumerate() {
            if flux_ty::fn_trait_kind(self.items, predicate.trait_ref.trait_).is_none() {
                continue;
            }
            if bounds[..index]
                .iter()
                .enumerate()
                .any(|(old_index, (old, old_self, old_rows))| {
                    let same = old == predicate
                        || (old.trait_ref == predicate.trait_ref
                            && keys[index].is_some()
                            && keys[index] == keys[old_index]);
                    same && (old_self != self_row || old_rows != rows)
                })
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "closure input bound occurrence is ambiguous",
                ));
                return false;
            }
        }
        true
    }

    /// The slots of an instance of `ty`, as [`Cx::bound_slots`], whose
    /// own are `table`, in terms of an item's lifetimes `lifetimes`.
    fn bound_slots_in(
        &mut self,
        ty: Ty,
        table: &[Region],
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        const HOLE: Var = Var::MAX;
        let late = self.late_inputs.clone();
        let mut map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => Some(lifetimes.get(i as usize).copied().unwrap_or(HOLE)),
            Region::Late(j) => Some(late.get(j as usize).copied().unwrap_or(HOLE)),
            Region::Erased => Some(HOLE),
        };
        let mut vars = self.instantiate_sig(ty, table, &mut map, args, arg_vars)?;
        for v in &mut vars {
            if *v == HOLE {
                *v = self.fresh();
                self.holes.insert(*v);
            }
        }
        Some(vars)
    }

    /// Maps the bound's own input slots through type substitution. Slots
    /// contributed by `T` are not higher-ranked slots: substituting `T =
    /// &u8` in `Fn(T, &u8) -> &u8` must still select the second input.
    fn late_slots(
        &mut self,
        ty: Ty,
        inputs: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
    ) -> Vec<Var> {
        fn positions(
            cx: &mut Cx<'_>,
            ty: Ty,
            args: &[Ty],
            arg_vars: &[Vec<Var>],
            offset: &mut usize,
            out: &mut Vec<usize>,
        ) {
            let own = match ty.kind() {
                TyKind::Param(p) => {
                    *offset += arg_vars.get(p.index as usize).map_or(0, Vec::len);
                    return;
                }
                TyKind::Alias(_) => {
                    let normalized = cx.normalize(ty.subst(args));
                    *offset += cx.slots(normalized) as usize;
                    return;
                }
                TyKind::Ref(..) | TyKind::Dynamic(_) => 1,
                TyKind::Adt(adt, _) => cx.items.structs[adt.strukt].generics.lifetimes.len(),
                TyKind::Opaque(o) => o.lifetimes as usize,
                _ => 0,
            };
            for _ in 0..own {
                out.push(*offset);
                *offset += 1;
            }
            match ty.kind() {
                TyKind::Ref(_, t)
                | TyKind::RawPtr(_, t)
                | TyKind::Array(t, _)
                | TyKind::Slice(t) => positions(cx, t, args, arg_vars, offset, out),
                TyKind::Tuple(ts) | TyKind::Adt(_, ts) => {
                    for &t in ts {
                        positions(cx, t, args, arg_vars, offset, out);
                    }
                }
                TyKind::Closure(_, list) => positions(
                    cx,
                    flux_ty::ClosureArgs::split(list).upvars,
                    args,
                    arg_vars,
                    offset,
                    out,
                ),
                TyKind::Opaque(o) => {
                    for &t in o.args {
                        positions(cx, t, args, arg_vars, offset, out);
                    }
                }
                _ => {}
            }
        }
        let mut own = Vec::new();
        positions(self, ty, args, arg_vars, &mut 0, &mut own);
        own.into_iter()
            .map(|i| inputs.get(i).copied().unwrap_or(Var::MAX))
            .collect()
    }

    /// Proves bound `p`, in the terms of an item whose lifetimes are
    /// `lifetimes` and generic arguments `args`, with slots `arg_vars`;
    /// the type it is on has slots `self_table`.
    #[allow(clippy::too_many_arguments)]
    fn prove_bound(
        &mut self,
        p: flux_ty::Predicate,
        self_table: &[Region],
        trait_regions: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        range: TextRange,
        depth: u32,
    ) {
        if let Some(goal) = self.prepare_bound(
            p,
            self_table,
            trait_regions,
            lifetimes,
            args,
            arg_vars,
            range,
            depth,
        ) {
            self.prove(
                goal.predicate,
                &goal.vars,
                &goal.lifetimes,
                goal.range,
                goal.depth,
            );
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn prepare_bound(
        &mut self,
        p: flux_ty::Predicate,
        self_table: &[Region],
        trait_regions: &TraitRefRegions,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        range: TextRange,
        depth: u32,
    ) -> Option<OrdinaryGoal> {
        // Function items fix their owner's and early-bound lifetimes when
        // instantiated. Fresh variables in function_output cannot prove
        // universality. The callback metadata includes own lifetime occurrences
        // anywhere in predicates, even alias arguments erased by normalization.
        // Explicit outlives bounds retain their conservative signature boundary;
        // this is not a general early/late partition or universe proof.
        // Keep this check even for unit/scalar outputs, before prove's
        // FnDef early return and projection fallback.
        if flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_some()
            && let TyKind::FnDef(def) = self.normalize(p.self_ty.subst(args)).kind()
        {
            let Some(regions) = self.sigs.regions.fns.get(def.function) else {
                self.errors.push(Diagnostic::new(
                    range,
                    "function item callback region metadata is not supported yet",
                ));
                return None;
            };
            if regions.callback_binder == flux_ty::regions::CallbackBinder::Unclassified {
                self.errors.push(Diagnostic::new(
                    range,
                    "function item callback region metadata is not supported yet",
                ));
                return None;
            }
            if regions.owner != 0
                || regions.callback_binder == flux_ty::regions::CallbackBinder::SignatureBound
            {
                let message = if regions.owner != 0 {
                    "higher-ranked callbacks with owner lifetimes are not supported yet"
                } else {
                    "higher-ranked callbacks with signature lifetime bounds are not supported yet"
                };
                let inputs = p.trait_ref.args.first().and_then(|&input| {
                    let table = trait_regions.args.first()?;
                    let inputs = self.bound_slots_in(input, table, lifetimes, args, arg_vars)?;
                    // Preserve the original input provenance: Param/Alias
                    // slots are fixed by the caller, not bound here.
                    Some(self.late_slots(input, &inputs, args, arg_vars))
                });
                let unrepresented = inputs.is_none_or(|inputs| {
                    inputs
                        .iter()
                        .any(|v| self.holes.contains(v) || self.late_inputs.contains(v))
                });
                if unrepresented {
                    let d = Diagnostic::new(range, message);
                    if !self.errors.contains(&d) {
                        self.errors.push(d);
                    }
                    return None;
                }
            }
        }
        // A closure's `Fn`-family bound: how the callee calls it.
        if flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_some()
            && let self_ty = self.normalize(p.self_ty.subst(args))
            && let TyKind::Closure(id, list) = self_ty.kind()
        {
            let n = self.slots(self_ty) as usize;
            let upvars = self
                .bound_slots_in(p.self_ty, self_table, lifetimes, args, arg_vars)
                .filter(|v| v.len() == n);
            let inputs = p.trait_ref.args.first().and_then(|&t| {
                let n = self.slots(self.normalize(t.subst(args))) as usize;
                let table = trait_regions.args.first().map_or(&[][..], |t| &**t);
                let old = std::mem::take(&mut self.late_inputs);
                let count = table
                    .iter()
                    .filter_map(|r| match r {
                        Region::Late(i) => Some(*i as usize + 1),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0);
                self.late_inputs = (0..count)
                    .map(|_| {
                        let v = self.fresh();
                        self.holes.insert(v);
                        v
                    })
                    .collect();
                let inputs = self
                    .bound_slots_in(t, table, lifetimes, args, arg_vars)
                    .filter(|v| v.len() == n);
                self.late_inputs = old;
                inputs
            });
            if let (Some(&ty), Some(inputs)) = (p.trait_ref.args.first(), &inputs) {
                let ty = self.normalize(ty.subst(args));
                let mut implied = Vec::new();
                flux_ty::regions::implied_outlives(self.items, self.sigs, ty, inputs, &mut implied);
                for relation in implied {
                    if let Outlives::Region(a, b) = relation
                        && self.holes.contains(&b)
                    {
                        self.hole_pairs.insert((a, b));
                    }
                }
            }
            // Prove the body against the original bound's output, including
            // fixed and static rows. Rebuilding only an erased Output alias
            // later cannot recover its original input/output lifetime link.
            let mut output = None;
            for (q, table) in &self.call_projections {
                if !self.is_fn_output(q.alias)
                    || q.alias.args.first() != Some(&p.self_ty)
                    || q.alias.args.get(1) != p.trait_ref.args.first()
                {
                    continue;
                }
                if output
                    .as_ref()
                    .is_some_and(|(old, row)| old != q || row != table)
                {
                    self.errors.push(Diagnostic::new(
                        range,
                        "closure output bound occurrence is ambiguous",
                    ));
                    return None;
                }
                output = Some((*q, table.clone()));
            }
            let ret = match (output, &inputs) {
                (Some((q, table)), Some(inputs)) => {
                    if table.len() != flux_ty::regions::slots(self.items, q.ty) as usize {
                        self.errors.push(Diagnostic::new(
                            range,
                            "closure output bound region rows do not match",
                        ));
                        return None;
                    }
                    let late = self.late_slots(p.trait_ref.args[0], inputs, args, arg_vars);
                    let old = std::mem::replace(&mut self.late_inputs, late);
                    let ret = self.bound_slots_in(q.ty, &table, lifetimes, args, arg_vars);
                    self.late_inputs = old;
                    let n = self.slots(self.normalize(q.ty.subst(args))) as usize;
                    match ret {
                        Some(row) if row.len() == n => Some(row),
                        _ => {
                            self.errors.push(Diagnostic::new(
                                range,
                                "closure output bound regions are not represented",
                            ));
                            return None;
                        }
                    }
                }
                _ => None,
            };
            self.closure_goal(
                id,
                list,
                upvars.as_deref(),
                inputs.as_deref(),
                ret.as_deref(),
                range,
            );
            return None;
        }
        if trait_regions.bound != 0 {
            let env = self.caller_region_goals();
            let goal = self.impl_region_goal_with_proof(
                p,
                self_table,
                trait_regions,
                lifetimes,
                args,
                arg_vars,
                Some((&env, depth)),
                false,
            );
            if goal
                .as_ref()
                .is_none_or(|goal| !self.impl_prove_super(goal, &env, depth))
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "higher-ranked trait lifetime obligation cannot be proved",
                ));
            }
            return None;
        }
        let abstract_goal = matches!(
            self.normalize(p.self_ty.subst(args)).kind(),
            TyKind::Param(_) | TyKind::Alias(_)
        ) && flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_none();
        let mut strict_map = |r| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        if abstract_goal
            && (trait_regions.lifetimes.len()
                != self.items.traits[p.trait_ref.trait_]
                    .generics
                    .lifetimes
                    .len()
                || trait_regions
                    .lifetimes
                    .iter()
                    .any(|&r| strict_map(r).is_none()))
        {
            self.errors.push(Diagnostic::new(
                range,
                "caller bound lifetime metadata is unavailable",
            ));
            return None;
        }
        let callable_rows = if flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_some() {
            Some(self.callable_output_rows(
                p,
                self_table,
                trait_regions,
                lifetimes,
                args,
                arg_vars,
                range,
            )?)
        } else {
            None
        };
        let mut header = Vec::with_capacity(1 + p.trait_ref.args.len());
        let mut vars = Vec::with_capacity(header.capacity());
        for (k, &t) in std::iter::once(&p.self_ty)
            .chain(p.trait_ref.args)
            .enumerate()
        {
            let table = if k == 0 {
                self_table
            } else {
                trait_regions.args.get(k - 1).map_or(&[][..], |t| t)
            };
            let slots = if let Some(rows) = &callable_rows {
                rows.get(k).cloned()
            } else if abstract_goal {
                self.instantiate_sig(t, table, &mut strict_map, args, arg_vars)
            } else {
                self.bound_slots_in(t, table, lifetimes, args, arg_vars)
            };
            let instance = self.normalize(t.subst(args));
            let Some(v) = slots.filter(|v| v.len() == self.slots(instance) as usize) else {
                if abstract_goal {
                    self.errors.push(Diagnostic::new(
                        range,
                        "caller bound lifetime metadata is unavailable",
                    ));
                }
                return None;
            };
            header.push(instance);
            vars.push(v);
        }
        let goal = flux_ty::Predicate {
            self_ty: header[0],
            trait_ref: flux_ty::TraitRef {
                args: Ty::list(&header[1..]),
                ..p.trait_ref
            },
        };
        let trait_lifetimes: Vec<Var> = trait_regions
            .lifetimes
            .iter()
            .map(|&r| match r {
                Region::Static => STATIC,
                Region::Param(i) => lifetimes
                    .get(i as usize)
                    .copied()
                    .unwrap_or_else(|| self.fresh()),
                Region::Erased | Region::Late(_) => self.fresh(),
            })
            .collect();
        Some(OrdinaryGoal {
            predicate: goal,
            vars,
            lifetimes: trait_lifetimes,
            range,
            depth,
        })
    }

    /// A rigid parameter or caller-declared region-free projection can use
    /// a unique generic-self blanket header. Every prerequisite is still
    /// proved; the wanted bound itself is never a caller assumption.
    fn caller_abstract_blanket(
        &mut self,
        goal: flux_ty::Predicate,
    ) -> Option<(flux_hir::ImplId, Vec<Ty>)> {
        if matches!(goal.self_ty.kind(), TyKind::Param(_)) {
            let known = self.caller_region_goals();
            let mut candidates = Vec::new();
            for imp in
                self.sigs
                    .impl_index
                    .impls_for(self.items, goal.trait_ref.trait_, goal.self_ty)
            {
                if !matches!(self.sigs.impls[imp].kind(), TyKind::Param(_)) {
                    continue;
                }
                let Some(mut args) =
                    flux_solve::traits::match_impl(self.items, self.sigs, imp, goal)
                else {
                    continue;
                };
                // Complete only a plain unknown parameter determined by an
                // original projection equation. Its owner inputs must already
                // be bound; no associated RHS is normalized or guessed.
                loop {
                    let mut changed = false;
                    for equation in self.sigs.impl_projections.get(imp).into_iter().flatten() {
                        let TyKind::Param(parameter) = equation.ty.kind() else {
                            continue;
                        };
                        let index = parameter.index as usize;
                        if !args.get(index).is_some_and(|arg| arg.has_error()) {
                            continue;
                        }
                        let projected = Ty::alias(equation.alias).subst(&args);
                        let TyKind::Alias(alias) = projected.kind() else {
                            continue;
                        };
                        if projected.has_error()
                            || flux_ty::regions::has_projection_inputs(self.items, alias)
                            || !self.items.traits[alias.trait_]
                                .generics
                                .lifetimes
                                .is_empty()
                            || alias
                                .args
                                .iter()
                                .any(|&ty| ty.has_aliases() || self.slots(ty) != 0)
                        {
                            continue;
                        }
                        let owner = flux_ty::Predicate {
                            self_ty: alias.args[0],
                            trait_ref: flux_ty::TraitRef {
                                trait_: alias.trait_,
                                name: alias.trait_name,
                                args: Ty::list(&alias.args[1..]),
                            },
                        };
                        if known.iter().any(|row| {
                            row.predicate == owner
                                && row.bound.is_empty()
                                && row.lifetimes.is_empty()
                                && row.vars.iter().all(Vec::is_empty)
                        }) {
                            args[index] = projected;
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }
                if args.iter().all(|arg| !arg.has_error()) {
                    candidates.push((imp, args));
                }
            }
            return (candidates.len() == 1).then(|| candidates.remove(0));
        }
        let TyKind::Alias(alias) = goal.self_ty.kind() else {
            return None;
        };
        if flux_ty::regions::has_projection_inputs(self.items, alias)
            || !self.items.traits[alias.trait_]
                .generics
                .lifetimes
                .is_empty()
            || alias
                .args
                .iter()
                .any(|&ty| ty.has_aliases() || self.slots(ty) != 0)
        {
            return None;
        }
        let owner = flux_ty::Predicate {
            self_ty: alias.args[0],
            trait_ref: flux_ty::TraitRef {
                trait_: alias.trait_,
                name: alias.trait_name,
                args: Ty::list(&alias.args[1..]),
            },
        };
        let known = self.caller_region_goals();
        if !known.iter().any(|known| {
            known.predicate == owner
                && known.bound.is_empty()
                && known.lifetimes.is_empty()
                && known.vars.iter().all(Vec::is_empty)
        }) {
            return None;
        }
        self.matching_impl(goal)
            .filter(|(imp, _)| matches!(self.sigs.impls[*imp].kind(), TyKind::Param(_)))
    }

    /// Try original caller predicates with their own region tables. Do not
    /// borrow metadata from an erased-equal predicate or from the callee.
    /// Closure bodies use the enclosing function's universal prefix, which
    /// closure region construction preserves as parent slots.
    fn caller_bound(
        &mut self,
        goal: flux_ty::Predicate,
        vars: &[Vec<Var>],
        trait_lifetimes: &[Var],
        range: TextRange,
    ) -> bool {
        if self.body.constant_owner.is_some() {
            let goal = ImplRegionGoal {
                predicate: goal,
                vars: vars.to_vec(),
                lifetimes: trait_lifetimes.to_vec(),
                bound: Vec::new(),
            };
            return self.impl_prove_super(&goal, &self.family_goals.clone(), 0);
        }
        let Some(f) = self.body.owner else {
            return false;
        };
        let Some(r) = self.sigs.regions.fns.get(f) else {
            return false;
        };
        let mut candidates = Vec::new();
        if let Some(flux_hir::Owner::Trait(owner)) = self.items.functions[f].owner
            && let Some(metadata) = self.sigs.regions.traits.get(owner)
            && metadata.names.len() == r.owner as usize
            && r.names.get(..metadata.names.len()) == Some(&metadata.names[..])
        {
            let predicate = flux_solve::traits::trait_self_predicate(self.items, owner);
            candidates.push((
                predicate,
                Box::default(),
                TraitRefRegions {
                    bound: 0,
                    lifetimes: (0..r.owner).map(Region::Param).collect(),
                    args: vec![Box::default(); predicate.trait_ref.args.len()].into(),
                },
            ));
        }
        if let Some(flux_hir::Owner::Impl(imp)) = self.items.functions[f].owner
            && let Some(block) = self.sigs.regions.impls.get(imp)
        {
            for (i, &p) in self
                .sigs
                .impl_predicates
                .get(imp)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let (Some(s), Some(t)) = (block.bound_selfs.get(i), block.bound_traits.get(i)) {
                    candidates.push((p, s.clone(), t.clone()));
                }
            }
        }
        for (i, &p) in self
            .sigs
            .fn_predicates
            .get(f)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let (Some(s), Some(t)) = (r.bound_selfs.get(i), r.bound_traits.get(i)) {
                candidates.push((p, s.clone(), t.clone()));
            }
        }
        if flux_ty::fn_trait_kind(self.items, goal.trait_ref.trait_).is_none() {
            let known = self.caller_region_goals();
            if let Some(equalities) =
                self.caller_goal_equalities(&known, goal, vars, trait_lifetimes)
            {
                for (a, b) in equalities {
                    self.equate(a, b, self.proving, range);
                }
                return true;
            }
        }
        for (p, self_table, tables) in candidates {
            let predicate = flux_ty::Predicate {
                self_ty: self.normalize(p.self_ty),
                trait_ref: flux_ty::TraitRef {
                    args: Ty::list(
                        &p.trait_ref
                            .args
                            .iter()
                            .map(|&t| self.normalize(t))
                            .collect::<Vec<_>>(),
                    ),
                    ..p.trait_ref
                },
            };
            let callable_supertrait = predicate.self_ty == goal.self_ty
                && predicate.trait_ref.args == goal.trait_ref.args
                && matches!(
                    (flux_ty::fn_trait_kind(self.items, predicate.trait_ref.trait_),
                     flux_ty::fn_trait_kind(self.items, goal.trait_ref.trait_)),
                    (Some(source), Some(target)) if source <= target
                );
            if (predicate != goal && !callable_supertrait)
                || tables.lifetimes.len() != trait_lifetimes.len()
                || tables.lifetimes.len()
                    != self.items.traits[p.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .len()
                || vars.len() != 1 + p.trait_ref.args.len()
            {
                continue;
            }
            let callable = flux_ty::fn_trait_kind(self.items, p.trait_ref.trait_).is_some();
            let count = tables.args.iter().map(|row| row.len()).sum::<usize>();
            let erased_vars = self.fresh_list(if callable { count as u32 } else { 0 });
            let mut erased = erased_vars.iter().copied();
            let mut late = FxHashMap::default();
            if callable {
                for &region in tables.args.iter().flatten() {
                    if let Region::Late(index) = region {
                        late.entry(index).or_insert_with(|| self.fresh());
                    }
                }
            }
            let mut map = |region| match region {
                Region::Static => Some(STATIC),
                Region::Param(i) if (i as usize) < r.names.len() => Some(1 + i),
                Region::Erased if callable => erased.next(),
                Region::Late(i) if callable => late.get(&i).copied(),
                _ => None,
            };
            let Some(lifetimes) = tables
                .lifetimes
                .iter()
                .map(|&r| map(r))
                .collect::<Option<Vec<_>>>()
            else {
                continue;
            };
            let mut equalities: Vec<_> = lifetimes
                .into_iter()
                .zip(trait_lifetimes.iter().copied())
                .collect();
            let mut complete = true;
            for (i, &ty) in std::iter::once(&p.self_ty)
                .chain(p.trait_ref.args)
                .enumerate()
            {
                let table = if i == 0 {
                    Some(&self_table)
                } else {
                    tables.args.get(i - 1)
                };
                let Some(known) =
                    table.and_then(|t| self.instantiate_sig(ty, t, &mut map, &[], &[]))
                else {
                    complete = false;
                    break;
                };
                if known.len() != vars[i].len() {
                    complete = false;
                    break;
                }
                equalities.extend(known.into_iter().zip(vars[i].iter().copied()));
            }
            // A required universal input cannot be supplied by fixing it to
            // a caller lifetime. Only original source binders may be freshly
            // instantiated at every invocation.
            if callable
                && equalities.iter().any(|&(source, wanted)| {
                    self.holes.contains(&wanted)
                        && !self.holes.contains(&source)
                        && !late.values().any(|&binder| binder == source)
                        && !erased_vars.contains(&source)
                })
            {
                continue;
            }
            let mut binder_targets = FxHashMap::default();
            if callable
                && equalities.iter().any(|&(source, wanted)| {
                    if !late.values().any(|&binder| binder == source)
                        && !erased_vars.contains(&source)
                    {
                        return false;
                    }
                    let Some(previous) = binder_targets.insert(source, wanted) else {
                        return false;
                    };
                    previous != wanted
                        && (self.holes.contains(&previous) || self.holes.contains(&wanted))
                        && !(self.hole_pairs.contains(&(previous, wanted))
                            && self.hole_pairs.contains(&(wanted, previous)))
                })
            {
                continue;
            }
            // Captures and inputs are universals of the closure body, not
            // the enclosing function. Export their required relationship to
            // the parent row; the closure's construction/call proves it.
            let exported = self.body.upvars.is_some();
            if !complete || (!exported && !self.caller_equalities_fit(&equalities)) {
                continue;
            }
            // These are requirements, never new header assumptions. The final
            // solver still checks constraints generated after this call.
            for (a, b) in equalities {
                self.equate(a, b, self.proving, range);
            }
            return true;
        }
        self.caller_family_bound(goal, vars, trait_lifetimes, range)
    }

    /// Match represented assumptions without adding an inconsistent candidate.
    fn caller_goal_equalities(
        &self,
        known: &[ImplRegionGoal],
        goal: flux_ty::Predicate,
        vars: &[Vec<Var>],
        trait_lifetimes: &[Var],
    ) -> Option<Vec<(Var, Var)>> {
        for scheme in known.iter().filter(|known| known.predicate == goal) {
            if scheme.lifetimes.len() != trait_lifetimes.len()
                || scheme.vars.len() != vars.len()
                || scheme
                    .vars
                    .iter()
                    .zip(vars)
                    .any(|(a, b)| a.len() != b.len())
            {
                continue;
            }
            let mut instantiated = FxHashMap::default();
            let mut equalities = Vec::new();
            for (a, b) in scheme
                .lifetimes
                .iter()
                .copied()
                .zip(trait_lifetimes.iter().copied())
                .chain(
                    scheme
                        .vars
                        .iter()
                        .zip(vars)
                        .flat_map(|(a, b)| a.iter().copied().zip(b.iter().copied())),
                )
            {
                if scheme.bound.contains(&a) {
                    if let Some(&old) = instantiated.get(&a) {
                        equalities.push((old, b));
                    } else {
                        instantiated.insert(a, b);
                    }
                } else {
                    equalities.push((a, b));
                }
            }
            // A closure exports requirements on parent rows, exactly as the
            // direct-bound path above. Its creator checks the relationships.
            if self.body.upvars.is_some() || self.caller_equalities_fit(&equalities) {
                return Some(equalities);
            }
        }
        None
    }

    fn trait_has_ranked_super(
        &self,
        id: flux_hir::TraitId,
        seen: &mut FxHashSet<flux_hir::TraitId>,
    ) -> bool {
        if !seen.insert(id) {
            return false;
        }
        self.sigs
            .regions
            .traits
            .get(id)
            .is_some_and(|r| r.supertraits.iter().any(|s| s.bound != 0))
            || self.sigs.traits.get(id).is_some_and(|s| {
                s.supertraits
                    .iter()
                    .any(|p| self.trait_has_ranked_super(p.trait_, seen))
            })
    }

    fn requires_lifetime_proof(
        &self,
        goal: flux_ty::Predicate,
        seen: &mut FxHashSet<flux_ty::Predicate>,
        depth: u32,
    ) -> bool {
        if !seen.insert(goal) {
            return false;
        }
        if depth > 8 {
            return true;
        }
        if self.trait_has_ranked_super(goal.trait_ref.trait_, &mut FxHashSet::default()) {
            return true;
        }
        if let Some(requirements) = self.structural_requirements(goal) {
            return requirements.into_iter().any(|required| {
                let required = flux_ty::Predicate {
                    self_ty: self.normalize(required.self_ty),
                    ..required
                };
                self.requires_lifetime_proof(required, seen, depth + 1)
            });
        }
        // An unresolved output must not hide a header's lifetime requirements.
        // Only consumers that bind rows require complete arguments.
        let selected = flux_solve::traits::RigidSolver::new(
            self.items,
            self.sigs,
            &self.header_predicates,
            &self.env,
            &self.unsized_params,
        )
        .candidate(goal);
        let Some((imp, args)) = selected else {
            return false;
        };
        let Some(regions) = self.sigs.regions.impls.get(imp) else {
            // A selected header without its conditions cannot justify a skip.
            return true;
        };
        if !regions.bounds.is_empty() || regions.bound_traits.iter().any(|t| t.bound != 0) {
            return true;
        }
        self.sigs.impl_predicates.get(imp).is_some_and(|preds| {
            preds
                .iter()
                .any(|p| self.requires_lifetime_proof(p.subst(&args), seen, depth + 1))
        })
    }

    /// Reconstruct original caller assumptions, including ordinary bounds
    /// needed by an impl that forwards a higher-ranked callback contract.
    fn caller_region_goals(&mut self) -> Vec<ImplRegionGoal> {
        if let Some(goals) = &self.caller_goals_in_progress {
            return goals.clone();
        }
        if self.body.constant_owner.is_some() {
            return self.family_goals.clone();
        }
        let Some(function) = self.body.owner else {
            return Vec::new();
        };
        let Some(regions) = self.sigs.regions.fns.get(function) else {
            return Vec::new();
        };
        let lifetimes = (0..regions.names.len())
            .map(|i| 1 + i as u32)
            .collect::<Vec<_>>();
        let mut written = Vec::new();
        match self.items.functions[function].owner {
            Some(flux_hir::Owner::Trait(id)) => {
                let predicate = flux_solve::traits::trait_self_predicate(self.items, id);
                let owner = regions.owner;
                written.push((
                    predicate,
                    Box::default(),
                    TraitRefRegions {
                        bound: 0,
                        lifetimes: (0..owner).map(Region::Param).collect(),
                        args: vec![Box::default(); predicate.trait_ref.args.len()].into(),
                    },
                ));
            }
            Some(flux_hir::Owner::Impl(id)) => {
                if let Some(block) = self.sigs.regions.impls.get(id) {
                    for (i, &p) in self
                        .sigs
                        .impl_predicates
                        .get(id)
                        .into_iter()
                        .flatten()
                        .enumerate()
                    {
                        if let (Some(row), Some(tr)) =
                            (block.bound_selfs.get(i), block.bound_traits.get(i))
                        {
                            written.push((p, row.clone(), tr.clone()));
                        }
                    }
                }
            }
            None => {}
        }
        for (i, &p) in self
            .sigs
            .fn_predicates
            .get(function)
            .into_iter()
            .flatten()
            .enumerate()
        {
            if let (Some(row), Some(tr)) = (regions.bound_selfs.get(i), regions.bound_traits.get(i))
            {
                written.push((p, row.clone(), tr.clone()));
            }
        }
        self.caller_goals_in_progress = Some(Vec::new());
        let goals = self.original_region_goals(written, &lifetimes);
        self.caller_goals_in_progress = None;
        self.close_known();
        goals
    }

    /// Build only original assumptions, retrying projections whose input
    /// memberships are written later in the declaration.
    fn original_region_goals(
        &mut self,
        mut pending: Vec<(flux_ty::Predicate, Box<[Region]>, TraitRefRegions)>,
        lifetimes: &[Var],
    ) -> Vec<ImplRegionGoal> {
        let caller = self.caller_goals_in_progress.is_some();
        let mut goals = Vec::new();
        while !pending.is_empty() {
            let count = pending.len();
            let mut remaining = Vec::new();
            for (predicate, row, traits) in pending {
                if let Some(goal) = self.impl_region_goal_with_inputs(
                    predicate,
                    &row,
                    &traits,
                    lifetimes,
                    &[],
                    &[],
                    None,
                    true,
                    Some(&goals),
                ) {
                    let _ = self.impl_super_goals(goal, &mut goals, 0);
                    self.family_goals = goals.clone();
                    if caller {
                        self.caller_goals_in_progress = Some(goals.clone());
                    }
                } else {
                    remaining.push((predicate, row, traits));
                }
            }
            if remaining.len() == count {
                break;
            }
            pending = remaining;
        }
        goals
    }

    /// Avoid choosing the first erased-equal candidate when its equality
    /// already contradicts the original header. Fresh call variables can be
    /// connected to universals through argument and earlier call constraints.
    fn caller_equalities_fit(&self, equalities: &[(Var, Var)]) -> bool {
        self.caller_equalities_fit_before(
            equalities,
            self.universal.len(),
            UniversalRelationMode::Rigid,
        )
    }

    /// Original rigid endpoints must retain their original relations. A scoped
    /// callback's placeholders have separate input-WF escape checks; inferred
    /// data slots in those inputs are not additional rigid universals. Scoped
    /// closure proofs defer only the same represented requirements exported by
    /// the final closure check; their creator/call must still discharge them.
    fn caller_equalities_fit_before(
        &self,
        equalities: &[(Var, Var)],
        universals: usize,
        mode: UniversalRelationMode,
    ) -> bool {
        let mut edges = vec![Vec::new(); self.vars as usize];
        for c in &self.constraints {
            edges[c.sup as usize].push(c.sub);
        }
        for &(a, b) in equalities {
            edges[a as usize].push(b);
            edges[b as usize].push(a);
        }
        for (i, &(a, _)) in self.universal.iter().enumerate() {
            for (j, &(b, _)) in self.universal.iter().enumerate() {
                if self.known(i as u32, j as u32) {
                    edges[a as usize].push(b);
                }
            }
        }
        for (i, &(start, _)) in self.universal.iter().take(universals).enumerate() {
            let mut seen = vec![false; self.vars as usize];
            let mut pending = vec![start];
            while let Some(v) = pending.pop() {
                if std::mem::replace(&mut seen[v as usize], true) {
                    continue;
                }
                if let Some(&j) = self.universal_of.get(&v)
                    && (j as usize) < universals
                    && !self.known(i as u32, j)
                    && !(matches!(mode, UniversalRelationMode::ClosureRequirements)
                        && self.closure_relation_exportable(i as u32, j))
                {
                    return false;
                }
                pending.extend(&edges[v as usize]);
            }
        }
        true
    }

    /// Instantiate only the exact written opaque bound rows. Captures carry
    /// owner lifetimes; type arguments carry their own substituted region rows.
    fn opaque_bound_goals(
        &mut self,
        opaque: flux_ty::OpaqueTy,
        own: &[Var],
    ) -> Option<Vec<ImplRegionGoal>> {
        if !self.opaque_bound_metadata(opaque) {
            return None;
        }
        let sig = self
            .sigs
            .opaques
            .get(opaque.function)?
            .get(opaque.index as usize)?;
        let owner = self.sigs.regions.fns.get(opaque.function)?;
        let decl = owner.opaques.get(opaque.index as usize)?;
        if decl.bounds.len() != sig.predicates.len()
            || sig.bound_origins.len() != sig.predicates.len()
            || decl.captures.len() != opaque.lifetimes as usize
        {
            return None;
        }
        let mut at = decl.captures.len();
        let mut arg_vars = Vec::new();
        for &ty in opaque.args {
            let count = self.slots(ty) as usize;
            arg_vars.push(own.get(at..at + count)?.to_vec());
            at += count;
        }
        if at != own.len() {
            return None;
        }
        arg_vars.push(own.to_vec());
        let args = flux_solve::traits::opaque_args(opaque);
        let env = self.caller_region_goals();
        let mut goals = Vec::new();
        for ((bound, predicate), origin) in decl
            .bounds
            .iter()
            .zip(sig.predicates.iter())
            .zip(sig.bound_origins.iter())
        {
            if bound.predicate != *predicate
                || bound.origin != *origin
                || bound.regions.args.len() != predicate.trait_ref.args.len()
                || bound.regions.lifetimes.len()
                    != self.items.traits[predicate.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .len()
            {
                return None;
            }
            let rows = std::iter::once(&bound.regions.lifetimes).chain(bound.regions.args.iter());
            for &region in rows.flatten() {
                if let Region::Param(_) = region
                    && !decl.captures.contains(&region)
                {
                    return None;
                }
            }
            let mut lifetimes = vec![STATIC; owner.names.len()];
            for (index, &capture) in decl.captures.iter().enumerate() {
                if let Region::Param(slot) = capture {
                    *lifetimes.get_mut(slot as usize)? = *own.get(index)?;
                }
            }
            let goal = self.impl_region_goal_with_inputs(
                *predicate,
                &[],
                &bound.regions,
                &lifetimes,
                &args,
                &arg_vars,
                None,
                false,
                Some(&env),
            )?;
            if !self.impl_super_goals(goal, &mut goals, 0) {
                return None;
            }
        }
        Some(goals)
    }

    /// Proves `goal`, whose self type and trait arguments have slots
    /// `vars`, by the `impl` block for its types, as rustc's trait
    /// selection does in the borrow checker: the block is chosen by
    /// types alone, then its header's lifetimes and its bounds, and
    /// those of the blocks its own bounds choose, constrain the goal's.
    /// Caller bounds on abstract types must prove the region contract too.
    #[allow(clippy::too_many_arguments)]
    fn prove_step(
        &mut self,
        goal: flux_ty::Predicate,
        vars: &[Vec<Var>],
        trait_lifetimes: &[Var],
        range: TextRange,
        depth: u32,
        parent: Option<usize>,
        work: &mut OrdinaryWork,
    ) {
        let (items, sigs) = (self.items, self.sigs);
        // Earlier obligations in this work frame can resolve a projection
        // after the goal's occurrence rows were prepared.
        let normalized = flux_ty::Predicate {
            self_ty: self.normalize(goal.self_ty),
            trait_ref: flux_ty::TraitRef {
                args: Ty::list(
                    &goal
                        .trait_ref
                        .args
                        .iter()
                        .map(|&ty| self.normalize(ty))
                        .collect::<Vec<_>>(),
                ),
                ..goal.trait_ref
            },
        };
        if normalized != goal {
            let types = std::iter::once(normalized.self_ty)
                .chain(normalized.trait_ref.args.iter().copied());
            if types
                .zip(vars)
                .any(|(ty, row)| self.slots(ty) as usize != row.len())
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "resolved trait goal region rows do not match",
                ));
                return;
            }
        }
        let goal = normalized;
        // An associated value may carry its owner's region rows while still
        // having a region-independent Sized contract from its declaration.
        if items.traits[goal.trait_ref.trait_].lang == Some(flux_intern::sym::sized)
            && flux_solve::traits::holds(items, sigs, &[], goal)
        {
            return;
        }
        if matches!(goal.self_ty.kind(), TyKind::Dynamic(object) if object.callable.is_some())
            && flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_some()
        {
            let requested = ImplRegionGoal {
                predicate: goal,
                vars: vars.to_vec(),
                lifetimes: trait_lifetimes.to_vec(),
                bound: Vec::new(),
            };
            let env = self.caller_region_goals();
            if !self.impl_prove_super(&requested, &env, depth) {
                self.errors.push(Diagnostic::new(
                    range,
                    "callable object input region requirements cannot be proved",
                ));
            }
            return;
        }
        let dynamic_object = matches!(goal.self_ty.kind(), TyKind::Dynamic(object)
            if flux_ty::fn_trait_kind(items, object.trait_).is_none());
        let pointer_callback = flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_some()
            && matches!(goal.self_ty.kind(), TyKind::FnPtr(sig) if sig.regions.is_some());
        // A callable may capture abstract types with zero region slots. Its
        // ordinary impl can still require those types to outlive a bound.
        let ordinary_callable = flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_none()
            && matches!(goal.self_ty.kind(), TyKind::Closure(..) | TyKind::FnDef(_));
        if !dynamic_object
            && !pointer_callback
            && !ordinary_callable
            && trait_lifetimes.is_empty()
            && vars.iter().all(|v| v.is_empty())
            && !self.requires_lifetime_proof(goal, &mut FxHashSet::default(), 0)
        {
            return;
        }
        if let TyKind::Opaque(opaque) = goal.self_ty.kind() {
            // Structural auto traits often depend only on captured types. A
            // hidden reference's region matters only when a selected impl
            // introduces an outlives requirement; retain the full proof then.
            if flux_solve::traits::is_auto_trait(items, goal.trait_ref.trait_)
                && !self.requires_lifetime_proof(goal, &mut FxHashSet::default(), 0)
                && flux_solve::traits::holds(items, sigs, &self.header_predicates, goal)
            {
                return;
            }
            let own = vars.first().map_or(&[][..], Vec::as_slice);
            let known = self.opaque_bound_goals(opaque, own);
            // Original callable bounds can introduce ranked universals. Their
            // complete relation must be closed before comparing memberships.
            self.close_known();
            let Some(known) = known else {
                self.errors.push(Diagnostic::new(
                    range,
                    "opaque bound region metadata cannot be resolved",
                ));
                return;
            };
            let wanted = ImplRegionGoal {
                predicate: goal,
                vars: vars.to_vec(),
                lifetimes: trait_lifetimes.to_vec(),
                bound: Vec::new(),
            };
            let satisfied = if known.iter().any(|bound| bound.predicate == goal) {
                if let Some(equalities) =
                    self.caller_goal_equalities(&known, goal, vars, trait_lifetimes)
                {
                    for (a, b) in equalities {
                        self.equate(a, b, self.proving, range);
                    }
                    true
                } else {
                    false
                }
            } else {
                let mut env = self.caller_region_goals();
                env.extend(known.iter().cloned());
                self.impl_prove_super(&wanted, &env, depth)
            };
            if !satisfied {
                self.errors.push(Diagnostic::new(
                    range,
                    "opaque bound lifetime obligation cannot be proved",
                ));
            }
            return;
        }
        if dynamic_object {
            let requested = ImplRegionGoal {
                predicate: goal,
                vars: vars.to_vec(),
                lifetimes: trait_lifetimes.to_vec(),
                bound: Vec::new(),
            };
            let Some(bounds) = self.object_super_goals(&requested) else {
                self.errors.push(Diagnostic::new(
                    range,
                    "trait object region metadata cannot be resolved",
                ));
                return;
            };
            if bounds.iter().any(|bound| bound.predicate == goal) {
                // Object formation does not establish every type-outlives
                // requirement of using its principal (for example Foo<T>:Any).
                if self.object_membership_outlives(&requested).is_none() {
                    self.errors.push(Diagnostic::new(
                        range,
                        "trait object membership lifetime metadata cannot be resolved",
                    ));
                    return;
                }
                let Some(equalities) =
                    self.caller_goal_equalities(&bounds, goal, vars, trait_lifetimes)
                else {
                    self.errors.push(Diagnostic::new(
                        range,
                        "trait object lifetime obligation cannot be proved",
                    ));
                    return;
                };
                for (a, b) in equalities {
                    self.equate(a, b, self.proving, range);
                }
                return;
            }
        }
        // Callable bounds may fix input rows to the enclosing method's
        // lifetimes, including inherited associated values. Check those rows
        // against the caller contract before using its abstract callable.
        if flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_some()
            && matches!(goal.self_ty.kind(), TyKind::Param(_) | TyKind::Alias(_))
        {
            if !self.caller_bound(goal, vars, trait_lifetimes, range) {
                self.errors.push(Diagnostic::new(
                    range,
                    "caller callable lifetime obligation cannot be proved",
                ));
            }
            return;
        }
        let mut caller_blanket = None;
        if matches!(goal.self_ty.kind(), TyKind::Param(_) | TyKind::Alias(_)) {
            if self.caller_bound(goal, vars, trait_lifetimes, range) {
                return;
            }
            caller_blanket = self.caller_abstract_blanket(goal);
            let represented_blanket = matches!(goal.self_ty.kind(),TyKind::Alias(alias)if flux_ty::regions::has_projection_inputs(self.items, alias))
                && (flux_solve::traits::select_impl(items, sigs, goal).is_some()
                    || self.family_blanket_impl(goal).is_some());
            let represented_blanket = represented_blanket || caller_blanket.is_some();
            if !represented_blanket {
                self.errors.push(Diagnostic::new(
                    range,
                    "caller bound lifetime obligation cannot be proved",
                ));
                return;
            }
        }
        if flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_some()
            && let TyKind::FnPtr(sig) = goal.self_ty.kind()
            && sig.regions.is_some()
        {
            let own = vars.first().map_or(&[][..], Vec::as_slice);
            let flat = vars.get(1).map_or(&[][..], Vec::as_slice);
            let Some(&tuple) = goal.trait_ref.args.first() else {
                return;
            };
            let TyKind::Tuple(input_types) = tuple.kind() else {
                return;
            };
            let mut inputs = Vec::new();
            let mut at = 0;
            for &ty in input_types {
                let n = self.slots(ty) as usize;
                let Some(row) = flat.get(at..at + n) else {
                    self.errors.push(Diagnostic::new(
                        range,
                        "function pointer callback region metadata is not supported yet",
                    ));
                    return;
                };
                inputs.push(row.to_vec());
                at += n;
            }
            if self
                .pointer_inputs(sig, own, input_types, &inputs, range)
                .is_none()
            {
                self.errors.push(Diagnostic::new(
                    range,
                    "function pointer callback region metadata is not supported yet",
                ));
            }
            return;
        }
        if self.structural_requirements(goal).is_some() {
            let wanted = ImplRegionGoal {
                predicate: goal,
                vars: vars.to_vec(),
                lifetimes: trait_lifetimes.to_vec(),
                bound: Vec::new(),
            };
            let env = self.caller_region_goals();
            if !self.impl_prove_super(&wanted, &env, depth) {
                self.errors.push(Diagnostic::new(
                    range,
                    "structural auto-trait lifetime obligation cannot be proved",
                ));
            }
            return;
        }
        // Caller predicates may constrain compound self types too, such as
        // `&'a Q: Into<K>`. Selecting a blanket impl first would invent its
        // prerequisites instead of using the contract the caller supplied.
        if self.caller_bound(goal, vars, trait_lifetimes, range) {
            return;
        }
        // Builtin callable contracts have dedicated closure/function proofs.
        // Ordinary blanket impls on these values still carry real lifetime
        // bounds (for example F: 'static), which must not be skipped.
        if matches!(goal.self_ty.kind(), TyKind::Infer(_))
            || (matches!(goal.self_ty.kind(), TyKind::Closure(..) | TyKind::FnDef(_))
                && flux_ty::fn_trait_kind(items, goal.trait_ref.trait_).is_some())
        {
            return;
        }
        let selected = flux_solve::traits::select_impl(items, sigs, goal);
        let Some((imp, impl_args)) = selected
            .as_ref()
            .filter(|(_, args)| args.iter().all(|ty| !ty.has_error()))
            .cloned()
            .or_else(|| self.matching_impl(goal))
            .or(selected)
            .or_else(|| self.family_blanket_impl(goal))
            .or(caller_blanket)
        else {
            if dynamic_object || self.requires_lifetime_proof(goal, &mut FxHashSet::default(), 0) {
                self.errors.push(Diagnostic::new(
                    range,
                    "trait lifetime impl selection cannot be resolved",
                ));
            }
            return;
        };
        let Some(regions) = sigs.regions.impls.get(imp) else {
            self.errors.push(Diagnostic::new(
                range,
                "impl lifetime metadata is unavailable",
            ));
            return;
        };
        if let Err(message) = work.check_expansion(goal, imp, parent) {
            self.errors.push(Diagnostic::new(range, message));
            return;
        }
        let header: Vec<Ty> = std::iter::once(goal.self_ty)
            .chain(goal.trait_ref.args.iter().copied())
            .collect();
        // These assumptions belong to the caller, never to the selected
        // required goal. Saved projection inputs may use earlier header
        // bindings to match a concrete associated output without inversion.
        let selection_env = self.caller_region_goals();
        let Some(binding) = self.bind_impl(
            imp,
            &impl_args,
            &header,
            vars,
            trait_lifetimes,
            range,
            depth,
            Some(&selection_env),
        ) else {
            self.errors.push(Diagnostic::new(
                range,
                "impl lifetime header cannot be instantiated",
            ));
            return;
        };
        // Lifetimes and types the header does not name are free.
        let lifetimes: Vec<Var> = binding
            .lifetimes
            .into_iter()
            .map(|l| l.unwrap_or_else(|| self.fresh()))
            .collect();
        let mut type_vars = Vec::with_capacity(binding.types.len());
        for (j, t) in binding.types.into_iter().enumerate() {
            let vars = match t {
                Some(vars) => vars,
                None => {
                    let n = self.slots(impl_args[j]);
                    self.fresh_list(n)
                }
            };
            type_vars.push(vars);
        }
        let map = |r: Region| match r {
            Region::Static => Some(STATIC),
            Region::Param(i) => lifetimes.get(i as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        for p in regions.bounds.iter() {
            match *p {
                Outlives::Region(a, b) => {
                    if let (Some(a), Some(b)) = (map(a), map(b)) {
                        self.outlives(a, b, self.proving, range);
                    } else {
                        self.errors.push(Diagnostic::new(
                            range,
                            "impl outlives condition is not represented",
                        ));
                    }
                }
                Outlives::Type(t, b) => {
                    let Some(b) = map(b) else {
                        self.errors.push(Diagnostic::new(
                            range,
                            "impl type-outlives condition is not represented",
                        ));
                        continue;
                    };
                    if let TyKind::Param(p) = t.kind()
                        && let Some(vars) = type_vars.get(p.index as usize)
                    {
                        for &v in &vars.clone() {
                            self.outlives(v, b, self.proving, range);
                        }
                    }
                    let instance = self.normalize(t.subst(&impl_args));
                    if instance.has_error() || instance.has_infer() {
                        self.errors.push(Diagnostic::new(
                            range,
                            "impl type-outlives subject is not represented",
                        ));
                    } else {
                        self.type_outlives(instance, b, range);
                    }
                }
            }
        }
        work.enqueue_impl(
            self, parent, goal, imp, lifetimes, impl_args, type_vars, range, depth,
        );
    }

    /// Proves what bound `p` of an item, with lifetimes `lifetimes`,
    /// generic arguments `args` and their slots `arg_vars`, says an
    /// associated type is (`I: Iterator<Item = &'a T>`), its slots
    /// `table`: the type the instance's normalizes to must be it,
    /// lifetimes and all.
    #[allow(clippy::too_many_arguments)]
    fn prove_projection(
        &mut self,
        p: flux_ty::traits::Projection,
        table: &[Region],
        input: Option<&ProjectionInputRecipe>,
        lifetimes: &[Var],
        args: &[Ty],
        arg_vars: &[Vec<Var>],
        range: TextRange,
    ) {
        if table.len() != flux_ty::regions::slots(self.items, p.ty) {
            self.errors.push(Diagnostic::new(
                range,
                "associated equality output metadata is incomplete",
            ));
            return;
        }
        let TyKind::Alias(alias) = Ty::alias(p.alias).subst(args).kind() else {
            return;
        };
        let actual_ty = self.normalize(Ty::alias(alias));
        let expected_ty = self.normalize(p.ty.subst(args));
        if !flux_solve::fn_regions::same_binders(self.items, actual_ty, expected_ty) {
            self.errors.push(Diagnostic::new(
                range,
                "associated function pointer binders do not match",
            ));
            return;
        }
        if table.is_empty() && self.slots(expected_ty) == 0 {
            return;
        }
        if self.is_fn_output(p.alias) {
            if let Some((predicate, own, rows)) = input {
                // Replay the complete original bound, not an erased alias
                // with fresh tuple slots. The proof keeps its input domain.
                let previous =
                    std::mem::replace(&mut self.call_projections, vec![(p, table.into())]);
                self.prove_bound(*predicate, own, rows, lifetimes, args, arg_vars, range, 0);
                self.call_projections = previous;
            } else if self.slots(expected_ty) != 0 {
                self.errors.push(Diagnostic::new(
                    range,
                    "callable output input occurrence is not represented",
                ));
            }
            return;
        }
        let mut alias_vars = Vec::with_capacity(p.alias.args.len());
        let mut projection_late = None;
        let actual = if p.alias.family_args.is_empty() && p.alias.family_lifetimes == 0 {
            if let Some(recipe) = input {
                let Some(goal) =
                    self.projection_input_goal(alias, recipe, lifetimes, args, arg_vars)
                else {
                    self.errors.push(Diagnostic::new(
                        range,
                        "associated equality input occurrence is not represented",
                    ));
                    return;
                };
                projection_late = Some(goal.bound);
                alias_vars = goal.vars;
                self.ordinary_projection_output(alias, &goal.lifetimes, &alias_vars)
            } else {
                // Parameter-only legacy projections carry their rows in the
                // generic arguments. Explicit references need source metadata.
                if p.alias.owner_lifetimes != 0
                    || p.alias.args.iter().any(|&ty| self.slots(ty) != 0)
                {
                    self.errors.push(Diagnostic::new(
                        range,
                        "associated equality input occurrence is not represented",
                    ));
                    return;
                }
                for &a in p.alias.args {
                    let Some(vars) = self.bound_slots(a, args, arg_vars) else {
                        return;
                    };
                    alias_vars.push(vars);
                }
                self.normalize_regions(alias, &alias_vars, 0)
            }
        } else {
            for &a in p.alias.args {
                let Some(vars) = self.bound_slots(a, args, arg_vars) else {
                    return;
                };
                alias_vars.push(vars);
            }
            self.instantiate_sig(Ty::alias(p.alias), &[], &mut |_| None, args, arg_vars)
        };
        let Some(actual) = actual else {
            self.errors.push(Diagnostic::new(
                range,
                "associated equality input regions are not supported yet",
            ));
            return;
        };
        let previous_late = self.late_inputs.clone();
        self.late_inputs = projection_late.unwrap_or_else(|| {
            p.alias.args.get(1).map_or(Vec::new(), |&ty| {
                self.late_slots(
                    ty,
                    alias_vars.get(1).map_or(&[][..], Vec::as_slice),
                    args,
                    arg_vars,
                )
            })
        });
        let expected = self.bound_slots_in(p.ty, table, lifetimes, args, arg_vars);
        self.late_inputs = previous_late;
        let Some(expected) = expected else {
            self.errors.push(Diagnostic::new(
                range,
                "associated equality output regions are not represented",
            ));
            return;
        };
        if actual.len() != expected.len() {
            self.errors.push(Diagnostic::new(
                range,
                "associated family equality region rows do not match",
            ));
        }
        if actual.len() == expected.len() {
            for (a, b) in actual.into_iter().zip(expected) {
                self.equate(a, b, Category::CallArgument, range);
            }
        }
    }

    /// Materialize stored associated fields while the hidden value's rows are
    /// available. A bare associated type cannot recover its output lifetimes.
    fn stored_type_outlives(
        &mut self,
        ty: Ty,
        vars: &[Var],
        bound: Var,
        category: Category,
        range: TextRange,
    ) {
        let hidden_ty = ty;
        let mut pending = vec![(ty, vars.to_vec(), Vec::<flux_hir::StructId>::new())];
        let mut seen = FxHashSet::default();
        let mut projected = None;
        while let Some((ty, vars, mut active)) = pending.pop() {
            if !seen.insert((ty, vars.clone(), active.clone())) {
                continue;
            }
            if vars.len() != self.slots(ty) as usize {
                self.type_outlives(ty, bound, range);
                continue;
            }
            for &var in &vars {
                self.outlives(var, bound, category, range);
            }
            let (children, mut at): (Vec<Ty>, usize) = match ty.kind() {
                TyKind::Adt(adt, args) => (
                    args.to_vec(),
                    self.items.structs[adt.strukt].generics.lifetimes.len(),
                ),
                TyKind::Tuple(types) => (types.to_vec(), 0),
                TyKind::Ref(_, inner) => (vec![inner], 1),
                TyKind::RawPtr(_, inner) | TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                    (vec![inner], 0)
                }
                TyKind::Closure(_, args) => (vec![flux_ty::ClosureArgs::split(args).upvars], 0),
                TyKind::FnPtr(signature) => signature
                    .regions
                    .map_or((Vec::new(), 0), |r| (r.types.to_vec(), r.free as usize)),
                _ => {
                    self.type_outlives(ty, bound, range);
                    continue;
                }
            };
            for child in children {
                let end = at + self.slots(child) as usize;
                let Some(row) = vars.get(at..end) else {
                    self.type_outlives(ty, bound, range);
                    break;
                };
                pending.push((child, row.to_vec(), active.clone()));
                at = end;
            }
            let TyKind::Adt(adt, _) = ty.kind() else {
                continue;
            };
            // Generic arguments remain obligations on recursive paths. Expanding
            // the same declaration's fields again would grow recursive containers.
            if active.contains(&adt.strukt)
                || !projected
                    .get_or_insert_with(|| {
                        flux_ty::regions::outlives_structs_with_projections(self.sigs, hidden_ty)
                    })
                    .contains(&adt.strukt)
            {
                continue;
            }
            active.push(adt.strukt);
            let variants = self
                .sigs
                .variant_starts
                .get(adt.strukt)
                .map_or(1, |v| v.len().saturating_sub(1));
            for variant in 0..variants {
                let fields = flux_solve::variant_fields(self.sigs, ty, variant as u32)
                    .map_or(0, |f| f.len());
                for field in 0..fields {
                    let (field_ty, field_vars) =
                        self.field_regions(ty, &vars, Some(variant as u32), field as u32);
                    if field_ty == Ty::ERROR {
                        self.type_outlives(ty, bound, range);
                    } else {
                        pending.push((field_ty, field_vars.to_vec(), active.clone()));
                    }
                }
            }
        }
    }

    /// `T: b` for the parameters and associated types in `ty`.
    fn type_outlives(&mut self, ty: Ty, b: Var, range: TextRange) {
        let mut pending = flux_ty::regions::outlives_type_components(self.sigs, ty);
        let mut seen = Vec::new();
        while let Some(component) = pending.pop() {
            if seen.contains(&component) {
                continue;
            }
            seen.push(component);
            let normalized = self.normalize(component);
            // Only erase an abstract test when normalization introduces no
            // reference slots. Their regions need the projection's input map,
            // which a bare Ty does not contain.
            if normalized != component && flux_ty::regions::slots(self.items, normalized) == 0 {
                pending.extend(flux_ty::regions::outlives_type_components(
                    self.sigs, normalized,
                ));
            } else {
                self.type_tests.push((component, b, range));
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
        if !self.closure_parent_regions(required, range) {
            return;
        }
        let (Some(closure), Some(params)) = (args.first(), args.get(1)) else {
            return self.opaque_call(args, dest, range);
        };
        let (self_ty, self_vars) = self.operand_regions(closure);
        let closure_ty = match self_ty.kind() {
            TyKind::Ref(_, inner) => inner,
            _ => self_ty,
        };
        let closure_args = match closure_ty.kind() {
            TyKind::Closure(_, args) => Some(args),
            _ => None,
        };
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
            Ext::Parent(_, k) => Some(1 + k),
            Ext::SelfRef => self_ref,
            Ext::Upvar(k) => upvars.get(k as usize).copied(),
            Ext::Arg(k) => arg_vars.get(k as usize).copied(),
            Ext::Ret(k) => dest_vars.get(k as usize).copied(),
        };
        let category = self.category_into(dest);
        if let Some(closure_args) = closure_args {
            self.closure_type_requirements(required, closure_args, false, map, range);
        } else if !required.type_outlives.is_empty() {
            self.errors.push(Diagnostic::new(
                range,
                "closure type arguments are not represented",
            ));
        }

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

    /// Whether `alias` is `<F as FnOnce<A>>::Output`.
    fn is_fn_output(&self, alias: AliasTy) -> bool {
        flux_ty::fn_trait_kind(self.items, alias.trait_) == Some(flux_ty::ClosureKind::FnOnce)
            && alias.name.as_str() == "Output"
    }

    /// Whether `alias` is a concrete callable's `Output`.
    fn is_callable_output(&self, alias: AliasTy) -> bool {
        matches!(
            alias.args[0].kind(),
            TyKind::Closure(..)
                | TyKind::FnDef(_)
                | TyKind::FnPtr(_)
                | TyKind::Opaque(_)
                | TyKind::Dynamic(_)
        ) && self.is_fn_output(alias)
    }

    /// Closure `id`, whose captures have slots `upvars` (unknown if
    /// `None`), meets a `Fn`-family bound whose parameters have slots
    /// `inputs` (`None` if they cannot be told): what its body requires
    /// of the lifetimes of its parameters and captures, as the bound
    /// says they are. Where the bound's own lifetimes (`F: Fn(&T)`, any
    /// lifetime at each call) would have to outlive, or be outlived by,
    /// anything else, the closure does not meet it, as in rustc. What
    /// it returns is related where its `Output` is normalized.
    fn closure_goal(
        &mut self,
        id: flux_ty::ClosureId,
        args: flux_ty::GenericArgs,
        upvars: Option<&[Var]>,
        inputs: Option<&[Var]>,
        ret: Option<&[Var]>,
        range: TextRange,
    ) {
        let requirements = self.requirements;
        let Some(required) = requirements.get(&id) else {
            return;
        };
        if !self.closure_parent_regions(required, range) {
            return;
        }
        if required
            .type_outlives
            .iter()
            .any(|&(_, target, _)| match target {
                Ext::Arg(k) => inputs.and_then(|rows| rows.get(k as usize)).is_none(),
                Ext::Ret(k) => ret.is_some_and(|rows| rows.get(k as usize).is_none()),
                Ext::SelfRef => true,
                _ => false,
            })
        {
            self.errors.push(Diagnostic::new(
                range,
                "closure type-outlives input target is not represented",
            ));
            return;
        }
        self.proven_closures.insert(id);
        if ret.is_some() {
            self.output_closures.insert(id);
        }
        let slot = |cx: &Cx<'_>, e: Ext| match e {
            Ext::Static => ClosureSlot::Var(STATIC),
            Ext::Parent(_, k) => ClosureSlot::Var(1 + k),
            Ext::Upvar(k) => match upvars.and_then(|u| u.get(k as usize)) {
                Some(&v) => ClosureSlot::Var(v),
                None => ClosureSlot::None,
            },
            Ext::Arg(k) => match inputs.map(|i| i.get(k as usize)) {
                None => ClosureSlot::Input,
                Some(Some(&v)) if cx.holes.contains(&v) => ClosureSlot::HigherRanked(v),
                Some(Some(&v)) => ClosureSlot::Var(v),
                Some(None) => ClosureSlot::None,
            },
            // What the bound says it returns, where its result has the
            // bound's own lifetimes; else related where its `Output` is
            // normalized.
            Ext::Ret(k) => match ret.and_then(|r| r.get(k as usize)) {
                Some(&v) if cx.holes.contains(&v) => ClosureSlot::HigherRanked(v),
                Some(&v) => ClosureSlot::Var(v),
                None => ClosureSlot::Ret,
            },
            Ext::SelfRef => ClosureSlot::None,
        };
        self.closure_type_requirements(
            required,
            args,
            true,
            |e| match e {
                Ext::Static => Some(STATIC),
                Ext::Parent(_, k) => Some(1 + k),
                Ext::Upvar(k) => upvars.and_then(|vars| vars.get(k as usize)).copied(),
                Ext::Arg(k) => inputs.and_then(|vars| vars.get(k as usize)).copied(),
                Ext::Ret(k) => ret.and_then(|vars| vars.get(k as usize)).copied(),
                Ext::SelfRef => None,
            },
            range,
        );
        for &(a, b, at) in &required.outlives {
            match (slot(self, a), slot(self, b)) {
                // A membership-only Fn<Args> obligation has no Output
                // equality. Result relations are checked when Output is used.
                (ClosureSlot::Ret, _) | (_, ClosureSlot::Ret) => {}
                (ClosureSlot::HigherRanked(x), ClosureSlot::HigherRanked(y))
                    if x == y || self.hole_pairs.contains(&(x, y)) => {}
                (ClosureSlot::Var(STATIC), ClosureSlot::HigherRanked(_)) => {}
                // What must outlive a bound's own lifetime must outlive
                // every lifetime allowed by its input type: forever
                // unless the input's well-formedness bounds it.
                (ClosureSlot::Var(x), ClosureSlot::HigherRanked(y)) => {
                    // `&T` bounds the placeholder by the lifetimes in T:
                    // `Fn(&&'short U) -> &U` may return that inner
                    // reference without requiring it to be static.
                    let upper: Vec<_> = self
                        .hole_pairs
                        .iter()
                        .filter(|&&(a, b)| b == y && !self.holes.contains(&a))
                        .map(|&(a, _)| a)
                        .collect();
                    if upper.is_empty() {
                        self.outlives(x, STATIC, Category::ClosureBounds, range);
                    } else {
                        for u in upper {
                            self.outlives(x, u, Category::ClosureBounds, range);
                        }
                    }
                }
                (ClosureSlot::HigherRanked(_), _) | (_, ClosureSlot::HigherRanked(_)) => {
                    let message = match b {
                        Ext::Upvar(_) => "borrowed data escapes outside of closure",
                        _ => "lifetime may not live long enough",
                    };
                    let d = Diagnostic::new(at, message);
                    if !self.errors.contains(&d) {
                        self.errors.push(d);
                    }
                }
                (ClosureSlot::Var(x), ClosureSlot::Var(y)) => {
                    self.outlives(x, y, Category::CallArgument, range);
                }
                (ClosureSlot::Input, ClosureSlot::Var(y)) => {
                    for x in self.call_inputs.clone() {
                        self.outlives(x, y, Category::CallArgument, range);
                    }
                }
                (ClosureSlot::Var(x), ClosureSlot::Input) => {
                    for y in self.call_inputs.clone() {
                        self.outlives(x, y, Category::CallArgument, range);
                    }
                }
                _ => {}
            }
        }
    }

    /// The slots of what closure `id`, with generic arguments `list`,
    /// returns when its captures have slots `upvars` and its
    /// parameters `inputs`: new regions, which what its body returns
    /// of them outlives.
    fn closure_output(
        &mut self,
        id: flux_ty::ClosureId,
        list: &'static [Ty],
        upvars: &[Var],
        inputs: &[Var],
    ) -> Vec<Var> {
        let ret = flux_ty::ClosureArgs::split(list).ret();
        let n = self.slots(ret);
        let outs = self.fresh_list(n);
        let range = self.call_range;
        let requirements = self.requirements;
        let Some(required) = requirements.get(&id) else {
            // A body not checked: it may return anything it is given.
            for &o in &outs {
                for &i in upvars.iter().chain(inputs) {
                    self.outlives(i, o, Category::CallArgument, range);
                }
            }
            return outs;
        };
        if !self.closure_parent_regions(required, range) {
            return outs;
        }
        self.output_closures.insert(id);
        let var = |e: Ext| match e {
            Ext::Static => Some(STATIC),
            Ext::Parent(_, k) => Some(1 + k),
            Ext::Upvar(k) => upvars.get(k as usize).copied(),
            Ext::Arg(k) => inputs.get(k as usize).copied(),
            Ext::Ret(k) => outs.get(k as usize).copied(),
            Ext::SelfRef => None,
        };
        self.closure_type_requirements(required, list, false, var, range);
        for &(a, b, _) in &required.outlives {
            if !matches!(a, Ext::Ret(_)) && !matches!(b, Ext::Ret(_)) {
                continue;
            }
            if let (Some(x), Some(y)) = (var(a), var(b)) {
                self.outlives(x, y, Category::CallArgument, range);
            }
        }
        outs
    }

    /// Named annotation lifetimes currently retain their enclosing signature
    /// identity. A different function needs a substitution table, never an
    /// accidental match by the same numeric lifetime-parameter index.
    fn closure_parent_regions(&mut self, required: &ClosureRequirements, range: TextRange) -> bool {
        let supported = required.outlives.iter().all(|&(a, b, _)| {
            [a, b].iter().all(|e| match e {
                Ext::Parent(owner, _) => Some(*owner) == self.body.owner,
                _ => true,
            })
        });
        let supported = supported && required.type_outlives.iter().all(|&(_, target, _)| {
            !matches!(target, Ext::Parent(owner, _) if Some(owner) != self.body.owner)
        });
        if !supported {
            let error = Diagnostic::new(
                range,
                "named closure annotation lifetimes across function boundaries are not supported yet",
            );
            if !self.errors.contains(&error) {
                self.errors.push(error);
            }
        }
        supported
    }

    /// The closures in a value of type `ty` with slots `vars`: each one's
    /// id and its captures' slots, into `out`.
    fn closures_in(
        &mut self,
        ty: Ty,
        vars: &[Var],
        next: &mut usize,
        out: &mut Vec<(flux_ty::ClosureId, flux_ty::GenericArgs, Vec<Var>)>,
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
            TyKind::Adt(adt, _)
                if flux_solve::is_lang_struct(self.items, adt.strukt, "coroutine") =>
            {
                // The closure is the future's private body, not a callback
                // independently passed to Pin or Future consumers. Its checked
                // output flows through the represented Future::Output type.
                *next += self.slots(ty) as usize;
            }
            TyKind::Adt(adt, list) => {
                *next += self.items.structs[adt.strukt].generics.lifetimes.len();
                for &t in list {
                    self.closures_in(t, vars, next, out);
                }
            }
            TyKind::Closure(id, args) => {
                let n = self.slots(ty) as usize;
                let own = vars.get(*next..*next + n).unwrap_or(&[]).to_vec();
                out.push((id, args, own));
                // Captured closures are private implementation details of this
                // callable. Its body summary composes their calls; treating
                // them as independent callee arguments invents result flows.
                *next += n;
            }
            TyKind::Opaque(_) => {
                // Its public bounds describe calls and result lifetimes.
                // Closures among the hidden implementation's type arguments
                // are not independently passed to a consumer of this value.
                *next += self.slots(ty) as usize;
            }
            _ => {}
        }
    }

    /// A closure, whose captures have slots `upvars`, passed to a call
    /// that may call it: what its body requires of what it captures.
    /// The callee's bounds say how it is called, which relates what it
    /// requires of its parameters and its result; where they did not,
    /// what it returns may hold anything the call is given, and what it
    /// keeps of its parameters, which flux cannot tell, is not
    /// supported where the callee's signature is unknown.
    #[allow(clippy::too_many_arguments)]
    fn passed_closure(
        &mut self,
        id: flux_ty::ClosureId,
        closure_args: flux_ty::GenericArgs,
        upvars: &[Var],
        args: &[Operand],
        dest: &Place,
        range: TextRange,
        signature_known: bool,
    ) {
        let Some(required) = self.requirements.get(&id) else {
            return;
        };
        self.closure_type_requirements(
            required,
            closure_args,
            true,
            |e| match e {
                Ext::Static => Some(STATIC),
                Ext::Parent(_, k) => Some(1 + k),
                Ext::Upvar(k) => upvars.get(k as usize).copied(),
                _ => None,
            },
            range,
        );
        let mut inputs = Vec::new();
        for op in args {
            inputs.extend(self.operand_regions(op).1);
        }
        let (_, dest_vars) = self.place_regions(dest);
        let category = self.category_into(dest);
        let proven = self.proven_closures.contains(&id);
        let output = self.output_closures.contains(&id);
        for &(_, target, at) in &required.type_outlives {
            let checked = match target {
                Ext::Arg(_) => proven,
                Ext::Ret(_) => output,
                Ext::SelfRef => false,
                _ => true,
            };
            if !checked {
                self.errors.push(Diagnostic::new(
                    at,
                    "closure type-outlives call target is not represented",
                ));
            }
        }
        for &(a, b, at) in &required.outlives {
            match (a, b) {
                (Ext::Arg(_), _) if proven => {}
                (Ext::Arg(_), _) if signature_known => {}
                (Ext::Arg(_), _) => {
                    let message =
                        "closures that keep references to their parameters are not supported yet";
                    let d = Diagnostic::new(at, message);
                    if !self.errors.contains(&d) {
                        self.errors.push(d);
                    }
                }
                (_, Ext::Ret(_)) if output => {}
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
        let category = self.category_into(dest);
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
            TyKind::Opaque(opaque) => {
                for _ in 0..opaque.lifetimes {
                    slot(next, enclosing, out);
                }
                for &t in opaque.args {
                    self.sinks(t, vars, next, behind_mut, enclosing, out);
                }
            }
            _ => {}
        }
    }

    /// The points where each local with slots is live: may be read or
    /// dropped later.
    fn liveness(&mut self, points: &Points, include_drops: bool) -> Vec<IntervalSet> {
        let body = self.body;
        let tracked: Vec<bool> = (0..body.locals.len())
            .map(|i| self.local_base[i + 1] > self.local_base[i])
            .collect();
        let held = (include_drops && body.has_unwind())
            .then(|| super::held_locals(body, self.sigs, &tracked));
        let live_out = super::liveness_where(body, &tracked, include_drops, held.as_deref());
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
            super::terminator_transfer_with_drops(
                &data.terminator.kind,
                &mut |l, v| set(l, v, at),
                include_drops,
            );
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

    /// An opaque value with an explicit outlives promise can be kept alive
    /// using that promise. Captured signature parameters that its hidden
    /// storage does not need must not extend unrelated argument borrows.
    fn storage_live_slots(&self, ty: Ty, out: &mut Vec<bool>) {
        if !ty.has_opaques() {
            let count = self
                .slot_counts
                .get(&ty)
                .copied()
                .map(|count| count as usize)
                .unwrap_or_else(|| flux_ty::regions::slots(self.items, ty));
            out.extend(std::iter::repeat_n(true, count));
            return;
        }
        match ty.kind() {
            TyKind::Ref(_, inner) => {
                out.push(true);
                self.storage_live_slots(inner, out);
            }
            TyKind::RawPtr(_, inner) | TyKind::Array(inner, _) | TyKind::Slice(inner) => {
                self.storage_live_slots(inner, out)
            }
            TyKind::Tuple(types) => {
                for &ty in types {
                    self.storage_live_slots(ty, out);
                }
            }
            TyKind::Adt(adt, args) => {
                out.extend(std::iter::repeat_n(
                    true,
                    self.items.structs[adt.strukt].generics.lifetimes.len(),
                ));
                for &ty in args {
                    self.storage_live_slots(ty, out);
                }
            }
            TyKind::Closure(_, args) => {
                self.storage_live_slots(flux_ty::ClosureArgs::split(args).upvars, out)
            }
            TyKind::Opaque(opaque) => {
                let decl = self
                    .sigs
                    .regions
                    .fns
                    .get(opaque.function)
                    .and_then(|regions| regions.opaques.get(opaque.index as usize));
                let promised = decl.filter(|decl| {
                    !decl.outlives.is_empty()
                        && decl.captures.len() == opaque.lifetimes as usize
                        && decl.outlives.iter().all(|region| {
                            *region == Region::Static || decl.captures.contains(region)
                        })
                });
                if let Some(decl) = promised {
                    out.extend(
                        decl.captures
                            .iter()
                            .map(|region| decl.outlives.contains(region)),
                    );
                    for &ty in opaque.args {
                        out.extend(std::iter::repeat_n(
                            false,
                            flux_ty::regions::slots(self.items, ty),
                        ));
                    }
                } else {
                    out.extend(std::iter::repeat_n(true, opaque.lifetimes as usize));
                    for &ty in opaque.args {
                        self.storage_live_slots(ty, out);
                    }
                }
            }
            _ => out.extend(std::iter::repeat_n(
                true,
                flux_ty::regions::slots(self.items, ty),
            )),
        }
    }

    /// Regions a destructor can inspect. A type with no Drop implementation
    /// delegates destruction to its fields; borrowed fields themselves do not
    /// inspect their referents. Unknown types and user destructors remain live.
    fn drop_live_vars(&mut self) -> FxHashSet<Var> {
        // Measure generic argument structure with a fixed budget. Descending
        // nested containers is finite, while a repeated declaration whose
        // arguments grow needs a conservative stop before expanding its fields.
        fn weight(ty: Ty, remaining: &mut usize) {
            if *remaining == 0 {
                return;
            }
            *remaining -= 1;
            match ty.kind() {
                TyKind::Ref(_, t)
                | TyKind::RawPtr(_, t)
                | TyKind::Array(t, _)
                | TyKind::Slice(t) => weight(t, remaining),
                TyKind::Tuple(args) | TyKind::Adt(_, args) | TyKind::Closure(_, args) => {
                    for &ty in args {
                        weight(ty, remaining);
                        if *remaining == 0 {
                            break;
                        }
                    }
                }
                _ => {}
            }
        }
        fn size(ty: Ty) -> usize {
            let mut remaining = 128;
            weight(ty, &mut remaining);
            128 - remaining
        }
        fn visit(
            cx: &mut Cx<'_>,
            ty: Ty,
            vars: &[Var],
            active: &mut Vec<Ty>,
            out: &mut FxHashSet<Var>,
        ) {
            let ty = cx.normalize(ty);
            match ty.kind() {
                TyKind::Ref(..) | TyKind::RawPtr(..) | TyKind::FnDef(_) | TyKind::FnPtr(_) => {}
                TyKind::Array(_, len) if len.as_usize() == Some(0) => {}
                TyKind::Array(elem, _) | TyKind::Slice(elem) => visit(cx, elem, vars, active, out),
                TyKind::Tuple(types) => {
                    let mut at = 0;
                    for &ty in types {
                        let n = cx.slots(ty) as usize;
                        visit(cx, ty, vars.get(at..at + n).unwrap_or(&[]), active, out);
                        at += n;
                    }
                }
                TyKind::Closure(_, args) => visit(
                    cx,
                    flux_ty::ClosureArgs::split(args).upvars,
                    vars,
                    active,
                    out,
                ),
                TyKind::Adt(adt, _)
                    if flux_solve::is_lang_struct(cx.items, adt.strukt, "manually_drop") => {}
                // PhantomData is an ownership declaration even though it has
                // no physical fields. Owned references have no destructor;
                // owned values with Drop keep the regions their Drop may read.
                TyKind::Adt(adt, args)
                    if flux_solve::is_lang_struct(cx.items, adt.strukt, "phantom_data") =>
                {
                    if let Some(&owned) = args.first() {
                        visit(cx, owned, vars, active, out);
                    }
                }
                TyKind::Adt(adt, args) => {
                    if let Some(imp) = flux_solve::traits::drop_impl(cx.items, cx.sigs, adt.strukt)
                    {
                        let generics = &cx.items.impls[imp].generics;
                        if !generics.params.iter().any(|p| p.may_dangle)
                            && !generics.lifetimes.iter().any(|p| p.may_dangle)
                        {
                            out.extend(vars.iter().copied());
                            return;
                        }
                        // A Drop impl's distinct header parameters map the
                        // promise to the instantiated owner's slots, including
                        // reordered lifetime and type parameters.
                        let lifetimes = cx.items.structs[adt.strukt].generics.lifetimes.len();
                        let header = cx.sigs.regions.impls.get(imp).map(|r| &*r.self_ty);
                        for (i, &var) in vars.iter().take(lifetimes).enumerate() {
                            let dangling = header.and_then(|r| r.get(i)).is_some_and(|r| {
                                matches!(r, Region::Param(p) if generics.lifetimes.get(*p as usize).is_some_and(|p| p.may_dangle))
                            });
                            if !dangling {
                                out.insert(var);
                            }
                        }
                        let template = cx.sigs.impls.get(imp).and_then(|t| match t.kind() {
                            TyKind::Adt(_, args) => Some(args),
                            _ => None,
                        });
                        let mut at = lifetimes;
                        for (i, &arg) in args.iter().enumerate() {
                            let n = cx.slots(arg) as usize;
                            let dangling = template.and_then(|args| args.get(i)).is_some_and(|t| {
                                matches!(t.kind(), TyKind::Param(p) if generics.params.get(p.index as usize).is_some_and(|p| p.may_dangle))
                            });
                            if !dangling {
                                out.extend(vars.get(at..at + n).unwrap_or(vars).iter().copied());
                            }
                            at += n;
                        }
                        // The unsafe promise does not suppress destruction of
                        // owned fields. Continue through actual storage below.
                    }
                    if active.contains(&ty) {
                        return;
                    }
                    // Exact type cycles terminate above. Growing generic
                    // instantiations also need a bound; preserve all obligations
                    // conservatively when declaration expansion does not stop.
                    let growing = active.iter().rev().find(|previous| {
                        matches!(previous.kind(), TyKind::Adt(previous, _) if previous.strukt == adt.strukt)
                    }).is_some_and(|&previous| size(ty) >= size(previous));
                    if growing || active.len() >= 64 {
                        out.extend(vars.iter().copied());
                        return;
                    }
                    active.push(ty);
                    let variants = cx
                        .sigs
                        .variant_starts
                        .get(adt.strukt)
                        .map_or(1, |v| v.len().saturating_sub(1));
                    for variant in 0..variants {
                        let fields = flux_solve::variant_fields(cx.sigs, ty, variant as u32)
                            .map_or(0, |f| f.len());
                        for field in 0..fields {
                            let (field_ty, field_vars) =
                                cx.field_regions(ty, vars, Some(variant as u32), field as u32);
                            if field_ty == Ty::ERROR {
                                out.extend(vars.iter().copied());
                            } else {
                                visit(cx, field_ty, &field_vars, active, out);
                            }
                        }
                    }
                    active.pop();
                }
                TyKind::Param(_)
                | TyKind::Alias(_)
                | TyKind::Opaque(_)
                | TyKind::Dynamic(_)
                | TyKind::Error => {
                    out.extend(vars.iter().copied());
                }
                _ => {}
            }
        }
        let mut live = FxHashSet::default();
        let locals: Vec<_> = self
            .local_base
            .windows(2)
            .enumerate()
            .map(|(i, w)| (i, w[0], w[1]))
            .collect();
        for (local, start, end) in locals {
            if start == end {
                continue;
            }
            let ty = self.body.locals[Local::from_raw(local as u32)].ty;
            let vars: Vec<_> = (start..end).collect();
            let before = self.vars;
            let mut found = FxHashSet::default();
            visit(self, ty, &vars, &mut Vec::new(), &mut found);
            // An unrepresented field map adds fresh slots tied to the whole
            // owner. Keep that owner conservative rather than lose obligations.
            if found.iter().any(|&v| v >= before) {
                live.extend(vars);
            } else {
                live.extend(found);
            }
        }
        live
    }

    /// Each region's value: its strongly connected component's, made of
    /// what its members are live at and the values of the components
    /// they outlive.
    fn solve(
        &mut self,
        points: &Points,
        liveness: &[IntervalSet],
        uses: &[IntervalSet],
    ) -> Solution {
        // Higher-ranked trait goals may introduce rigid regions while building
        // body constraints, after the initial signature relation was closed.
        self.close_known();
        for PointerCheck {
            before,
            bound,
            implied,
            range,
            ..
        } in std::mem::take(&mut self.pointer_checks)
        {
            let universal = self.universal.len();
            let known = self.known_pairs.len();
            for &var in &bound {
                if !self.universal_of.contains_key(&var) {
                    self.universal_of.insert(var, self.universal.len() as u32);
                    self.universal.push((var, "'pointer_bound".into()));
                }
            }
            // An inferred caller slot can bound the admitted input domain
            // without becoming a universal assumption. The full WF graph
            // remains available to the scoped proof below.
            self.known_pairs
                .extend(implied.iter().copied().filter(|(a, b)| {
                    self.universal_of.contains_key(a) && self.universal_of.contains_key(b)
                }));
            self.close_known();
            self.finish_pointer_proof(before, &bound, &implied, universal, known, range);
        }
        let drop_live = self.drop_live_vars();
        let n = self.vars as usize;
        // The successors of each variable: what it outlives.
        let edges = Adjacency::new(n, self.constraints.iter().map(|c| (c.sup, c.sub)));
        self.by_sup = Adjacency::new(
            n,
            self.constraints
                .iter()
                .enumerate()
                .map(|(i, c)| (c.sup, i as u32)),
        );
        let mut scratch = Vec::new();
        let everywhere = IntervalSet::full(points.total);
        let (scc, order) = tarjan(&edges);
        let count = order.len();
        let mut values = vec![Value::default(); count];
        // Each component's own: its locals' liveness and its universal
        // regions'.
        let mut var_local = vec![u32::MAX; n];
        for (local, w) in self.local_base.windows(2).enumerate() {
            let mut live = Vec::new();
            self.storage_live_slots(
                self.body.locals[Local::from_raw(local as u32)].ty,
                &mut live,
            );
            debug_assert_eq!(live.len(), (w[1] - w[0]) as usize);
            for (v, needed) in (w[0]..w[1]).zip(live) {
                if needed {
                    var_local[v as usize] = local as u32;
                }
            }
        }
        for v in 0..n {
            let c = scc[v] as usize;
            if let Some(&u) = self.universal_of.get(&(v as u32)) {
                values[c]
                    .points
                    .union_with_scratch(&everywhere, &mut scratch);
                if !values[c].ends.contains(&u) {
                    values[c].ends.push(u);
                }
            }
            let local = var_local[v];
            if local != u32::MAX {
                values[c].points.union_with_scratch(
                    if drop_live.contains(&(v as Var)) {
                        &liveness[local as usize]
                    } else {
                        &uses[local as usize]
                    },
                    &mut scratch,
                );
            }
        }
        // The members of each component, for its successors.
        let members = Adjacency::new(count, scc.iter().enumerate().map(|(v, &c)| (c, v as u32)));
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
                    value
                        .points
                        .union_with_scratch(&values[s].points, &mut scratch);
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

    /// Fixed signature data and parent/capture rows form requirements checked
    /// by the closure's creator/call. Its own borrow and independently borrowed
    /// annotated inputs remain rigid. Unrepresented endpoints cannot export.
    fn closure_relation_exportable(&self, source: u32, target: u32) -> bool {
        let Some(upvars) = &self.body.upvars else {
            return false;
        };
        let (Some(&source), Some(_)) = (
            self.closure_ext.get(source as usize),
            self.closure_ext.get(target as usize),
        ) else {
            return false;
        };
        let own_arg = matches!(source, Ext::Arg(k) if !upvars.fixed_args.contains(&k))
            && upvars.annotated_refs;
        source != Ext::SelfRef && !own_arg
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
                    let target = self.universal[b as usize].0;
                    let (_, range) = self.blame(var, target);
                    if self.closure_relation_exportable(a, b) {
                        self.own_requirements.push((
                            self.closure_ext[a as usize],
                            self.closure_ext[b as usize],
                            range,
                        ));
                        continue;
                    }
                    let message = if self.closure_ext.get(a as usize) == Some(&Ext::SelfRef)
                        && upvars.kind == flux_ty::ClosureKind::FnMut
                    {
                        "captured variable cannot escape `FnMut` closure body"
                    } else {
                        "lifetime may not live long enough"
                    };
                    errors.push(Diagnostic::new(range, message));
                    continue;
                }
                let target = self.universal[b as usize].0;
                let (category, range) = self.blame(var, target);
                // rustc's E0521: a parameter's lifetime that a call needs
                // to be `'static`.
                let message = if b == 0
                    && category == Category::CallArgument
                    && self.in_params.contains(&a)
                    && self.body.owner.is_some()
                {
                    "borrowed data escapes outside of function"
                } else {
                    "lifetime may not live long enough"
                };
                errors.push(Diagnostic::new(range, message));
                // One error per lifetime, as rustc reports.
                break;
            }
        }
        errors
    }

    /// The parameters and associated types that must outlive universal
    /// regions they are not known to.
    fn check_type_tests(&mut self, solution: &Solution) -> Vec<Diagnostic> {
        let mut errors = Vec::new();
        for &(ty, var, range) in &self.type_tests {
            let value = &solution.values[solution.scc[var as usize] as usize];
            for &e in &value.ends {
                if self.known_type_outlives(ty, e, &mut Vec::new()) {
                    continue;
                }
                // A closure may require a known input type bound to flow
                // through one of its exported region relations. That relation
                // is checked at its creator/call, never assumed unconditionally.
                if self.body.upvars.is_some()
                    && self
                        .universal
                        .iter()
                        .enumerate()
                        .any(|(origin, &(var, _))| {
                            let origin = origin as u32;
                            let relation_exported = self.known(origin, e)
                                || self.own_requirements.iter().any(|&(a, b, _)| {
                                    a == self.closure_ext[origin as usize]
                                        && b == self.closure_ext[e as usize]
                                });
                            relation_exported
                                && solution.values[solution.scc[var as usize] as usize]
                                    .ends
                                    .contains(&e)
                                && self.known_type_outlives(ty, origin, &mut Vec::new())
                        })
                {
                    continue;
                }
                if self.body.upvars.is_some() {
                    let external = self.closure_ext[e as usize];
                    let requirement = (ty, external, range);
                    if !self.own_type_requirements.contains(&requirement) {
                        self.own_type_requirements.push(requirement);
                    }
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

    /// A projection outlives a region if all its inputs do. Lifetime-bearing
    /// trait references or type arguments need their represented region maps;
    /// do not infer anything from erased inputs in those cases.
    fn known_type_outlives(&self, ty: Ty, end: u32, active: &mut Vec<Ty>) -> bool {
        // Lifetime-bearing projections need their occurrence's concrete rows.
        // Type/const-only families can use type facts when all their arguments
        // have zero slots; no occurrence regions are erased in that case.
        if matches!(ty.kind(), TyKind::Alias(alias)
            if flux_ty::regions::has_projection_inputs(self.items, alias)
                && (alias.owner_lifetimes != 0 || alias.family_lifetimes != 0
                    || alias.template_args().iter().any(|&arg| flux_ty::regions::slots(self.items, arg) != 0)))
        {
            return false;
        }
        let normalized = self.normalize(ty);
        // An inherited projection can forward through another associated
        // type. A signature's implied bound applies to that same normalized
        // type, while selection/header lifetime obligations remain checked
        // by the projection recipes. Do not infer equality from erased
        // region-bearing projection inputs.
        let represented = match normalized.kind() {
            TyKind::Alias(alias) => {
                alias.owner_lifetimes == 0
                    && alias.family_lifetimes == 0
                    && self.items.traits[alias.trait_]
                        .generics
                        .lifetimes
                        .is_empty()
                    && alias
                        .template_args()
                        .iter()
                        .all(|&arg| flux_ty::regions::slots(self.items, arg) == 0)
            }
            _ => flux_ty::regions::slots(self.items, normalized) == 0,
        };
        if self.known_types.iter().any(|&(t, b)| {
            (t == ty || (represented && self.normalize(t) == normalized))
                && self.known(self.universal_of[&b], end)
        }) {
            return true;
        }
        let TyKind::Alias(alias) = ty.kind() else {
            return false;
        };
        // A declared static output does not depend on the owner's lifetime.
        // Selection and associated-value checks enforce this contract at its
        // definition; using it must not require the owner itself to be static.
        if self
            .sigs
            .regions
            .traits
            .get(alias.trait_)
            .and_then(|contract| contract.families.iter().find(|(name, _)| *name == alias.name))
            .is_some_and(|(_, output)| {
                output.output_outlives.contains(&Region::Static)
                    || output.output_traits.iter().any(|(bound, _)| {
                        self.sigs.regions.traits.get(bound.trait_).is_some_and(|contract| {
                            contract.outlives.iter().any(|requirement| {
                                matches!(requirement, Outlives::Type(ty, Region::Static)
                                    if matches!(ty.kind(), TyKind::Param(param) if param.index == 0))
                            })
                        })
                    })
            })
        {
            return true;
        }
        if active.contains(&ty)
            || !self.items.traits[alias.trait_]
                .generics
                .lifetimes
                .is_empty()
            || alias.args.is_empty()
            || alias
                .template_args()
                .iter()
                .any(|&arg| flux_ty::regions::slots(self.items, arg) != 0)
        {
            return false;
        }
        active.push(ty);
        let holds = alias.template_args().iter().all(|&arg| {
            flux_ty::regions::outlives_type_components(self.sigs, arg)
                .into_iter()
                .all(|component| self.known_type_outlives(component, end, active))
        });
        active.pop();
        holds
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
        if loans.is_empty() {
            return Vec::new();
        }
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
        let mut visited = BitSet::new_empty(body.blocks.len());
        let mut touched: Vec<usize> = Vec::new();
        let mut scopes = Vec::with_capacity(loans.len());
        let mut runs: Vec<(u32, u32)> = Vec::new();
        let mut stack: Vec<(usize, u32)> = Vec::new();
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
            runs.clear();
            stack.push((
                loan.block.index(),
                points.point(loan.block, loan.statement + 1),
            ));
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
                    for succ in body.blocks[BasicBlock::from_raw(block as u32)]
                        .terminator
                        .successors()
                    {
                        let succ = succ.index();
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
            for &(s, e) in &runs {
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
fn tarjan(edges: &Adjacency) -> (Vec<u32>, Vec<u32>) {
    let n = edges.offsets.len() - 1;
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

/// Compressed adjacency, preserving input order within each row.
#[derive(Default)]
pub(super) struct Adjacency {
    offsets: Vec<usize>,
    values: Vec<u32>,
}

impl Adjacency {
    pub(super) fn new(nodes: usize, edges: impl Iterator<Item = (u32, u32)> + Clone) -> Self {
        let mut offsets = vec![0; nodes + 1];
        for (from, _) in edges.clone() {
            offsets[from as usize + 1] += 1;
        }
        for i in 0..nodes {
            offsets[i + 1] += offsets[i];
        }
        let mut values = vec![0; offsets[nodes]];
        let mut cursor = offsets[..nodes].to_vec();
        for (from, to) in edges {
            values[cursor[from as usize]] = to;
            cursor[from as usize] += 1;
        }
        Self { offsets, values }
    }
}

impl std::ops::Index<usize> for Adjacency {
    type Output = [u32];
    fn index(&self, node: usize) -> &[u32] {
        &self.values[self.offsets[node]..self.offsets[node + 1]]
    }
}

#[cfg(test)]
mod graph_tests {
    use super::*;

    #[test]
    fn temporary_slots_keep_short_lists_inline_and_spill_without_truncation() {
        let items = ItemTree::default();
        let sigs = Signatures::default();
        let requirements = Requirements::default();
        let reference = Ty::new(TyKind::Ref(Mutability::Not, Ty::UNIT));
        for n in [0, 1, 4, 5, 65_536] {
            let ty = Ty::tuple(&vec![reference; n]);
            let mut body = Body::empty(None);
            let local = body.locals.alloc(crate::LocalDecl {
                ty,
                kind: crate::LocalKind::Return,
            });
            let mut cx = Cx::new(&body, &items, &sigs, &requirements);
            let base = 70_000;
            cx.local_base = vec![base, base + n as u32];
            let vars = cx.local_vars(local);
            assert_eq!(vars.len(), n);
            assert_eq!(vars.spilled(), n > 4);
            assert!(vars.iter().copied().eq(base..base + n as u32));
            let (_, slots) = cx.place_regions(&local.into());
            assert_eq!(slots, vars);
            if n > 0 {
                let (field_ty, slots) = cx.place_regions(&Place::from(local).field(n as u32 - 1));
                assert_eq!(field_ty, reference);
                assert_eq!(&slots[..], &[base + n as u32 - 1]);
                assert!(!slots.spilled());
                let (_, slots) = cx.place_regions(&Place::from(local).field(0).deref());
                assert!(slots.is_empty());
                assert!(!slots.spilled());
            }
            // Field instantiation writes directly into inline storage,
            // including a large spill.
            let mut instantiated = Slots::new();
            let own = vec![Region::Static; n];
            assert!(cx.inst(
                ty,
                &own,
                &mut 0,
                &mut |_| Some(STATIC),
                &[],
                &[] as &[&[Var]],
                &mut instantiated,
            ));
            assert_eq!(instantiated.len(), n);
            assert_eq!(instantiated.spilled(), n > 4);
            assert!(instantiated.iter().all(|&v| v == STATIC));
        }
    }

    #[test]
    fn flat_graph_preserves_order_and_components() {
        let graph = Adjacency::new(
            6,
            [(0, 1), (0, 2), (1, 3), (2, 3), (3, 1), (4, 4)].into_iter(),
        );
        assert_eq!(&graph[0], &[1, 2]);
        assert!(graph[5].is_empty());
        let (scc, order) = tarjan(&graph);
        assert_eq!(scc[1], scc[3]);
        assert_ne!(scc[0], scc[2]);
        assert_ne!(scc[4], scc[5]);
        assert_eq!(order.len(), 5);
        for from in 0..6 {
            for &to in &graph[from] {
                assert!(scc[to as usize] <= scc[from]);
            }
        }
    }

    #[test]
    fn flat_graph_handles_long_cycles_without_recursion() {
        let n = 20_000;
        let graph = Adjacency::new(n, (0..n as u32).map(|i| (i, (i + 1) % n as u32)));
        let (scc, order) = tarjan(&graph);
        assert_eq!(order, [0]);
        assert!(scc.iter().all(|&c| c == 0));
    }
    #[test]
    fn structural_auto_supertraits_keep_selected_impl_outlives() {
        for projection in [false, true] {
            for (bound, expected) in [("", false), (":'static", true)] {
                let field = if projection {
                    "<Source<T>as Family>::Out"
                } else {
                    "Guard<T>"
                };
                let source = format!(
                    "struct Guard<T>(core::marker::PhantomData<T>);unsafe impl<T:'static>Send for Guard<T>{{}}trait Family{{type Out;}}struct Source<T>(core::marker::PhantomData<T>);impl<T>Family for Source<T>{{type Out=Guard<T>;}}struct W<T>({field});trait Typed:Send{{}}impl<T{bound}>Typed for W<T>{{}}fn main(){{}}"
                );
                let (_, _, hir) = flux_hir::lower_text("selected_auto_lifetime.rs", &source);
                assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
                let (sigs, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
                assert!(diagnostics.is_empty(), "{diagnostics:?}");
                let imp = hir
                    .items
                    .impls
                    .iter()
                    .find(|(id, _)| {
                        sigs.impl_traits
                            .get(*id)
                            .is_some_and(|t| t.name.as_str() == "Typed")
                    })
                    .unwrap()
                    .0;
                // The admitted static prerequisite must remain an obligation
                // of the selected impl, including through a projected field.
                assert_eq!(
                    impl_supertraits_match(&hir.items, &sigs, imp),
                    Some(expected),
                    "projection={projection}, bound={bound}"
                );
            }
        }
    }
}
