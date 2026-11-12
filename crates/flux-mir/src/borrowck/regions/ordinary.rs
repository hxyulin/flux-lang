//! Depth-first ordinary impl prerequisites without recursive Rust stack frames.
use super::*;

// A finite work budget also covers non-embedding recurrences.
const MAX_ORDINARY_FRAMES: usize = 4096;
const EMBEDDING_DEPTH_LIMIT: u32 = 64;

pub(super) struct OrdinaryGoal {
    pub predicate: flux_ty::Predicate,
    pub vars: Vec<Vec<Var>>,
    pub lifetimes: Vec<Var>,
    pub range: TextRange,
    pub depth: u32,
}

#[derive(Clone)]
struct Context {
    projections: Vec<(flux_ty::Projection, Box<[Region]>)>,
    late_inputs: Vec<Var>,
    range: TextRange,
    category: Category,
}
impl Context {
    fn capture(cx: &Cx<'_>) -> Self {
        Self {
            projections: cx.call_projections.clone(),
            late_inputs: cx.late_inputs.clone(),
            range: cx.call_range,
            category: cx.proving,
        }
    }
    fn install(self, cx: &mut Cx<'_>) -> Self {
        Self {
            projections: std::mem::replace(&mut cx.call_projections, self.projections),
            late_inputs: std::mem::replace(&mut cx.late_inputs, self.late_inputs),
            range: std::mem::replace(&mut cx.call_range, self.range),
            category: std::mem::replace(&mut cx.proving, self.category),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    predicate: flux_ty::Predicate,
    vars: Vec<Vec<Var>>,
    lifetimes: Vec<Var>,
    projections: Vec<(flux_ty::Projection, Box<[Region]>)>,
    late_inputs: Vec<Var>,
    range: TextRange,
}
impl Key {
    fn new(goal: &OrdinaryGoal, context: &Context) -> Self {
        Self {
            predicate: goal.predicate,
            vars: goal.vars.clone(),
            lifetimes: goal.lifetimes.clone(),
            projections: context.projections.clone(),
            late_inputs: context.late_inputs.clone(),
            range: goal.range,
        }
    }
}

struct Frame {
    predicate: flux_ty::Predicate,
    imp: flux_hir::ImplId,
    size: usize,
    parent: Option<usize>,
    lifetimes: Vec<Var>,
    args: Vec<Ty>,
    vars: Vec<Vec<Var>>,
    range: TextRange,
    depth: u32,
    outer: Context,
    bound: Context,
}

enum Task {
    Goal(OrdinaryGoal, Context, Option<usize>),
    Bound { frame: usize, index: usize },
    Projection { frame: usize, index: usize },
    Complete(Key, usize),
}

pub(super) struct OrdinaryWork {
    pending: Vec<Task>,
    frames: Vec<Frame>,
    proven: FxHashSet<Key>,
}
impl OrdinaryWork {
    /// Ordinary traits are inductive. Repeated goals cannot prove themselves,
    /// and a structurally growing recurrence cannot provide a finite proof.
    pub(super) fn check_expansion(
        &self,
        predicate: flux_ty::Predicate,
        imp: flux_hir::ImplId,
        mut parent: Option<usize>,
    ) -> Result<(), &'static str> {
        if self.frames.len() >= MAX_ORDINARY_FRAMES {
            return Err("trait lifetime obligation limit exceeded");
        }
        let size = goal_size(predicate);
        while let Some(index) = parent {
            let frame = &self.frames[index];
            if frame.predicate == predicate {
                return Err("trait lifetime obligation cycle cannot be proved");
            }
            if frame.imp == imp
                && size > frame.size
                && !strict_subterm(predicate.self_ty, frame.predicate.self_ty)
                && embeds(frame.predicate.self_ty, predicate.self_ty, 0)
                && frame.predicate.trait_ref.args.len() == predicate.trait_ref.args.len()
                && frame
                    .predicate
                    .trait_ref
                    .args
                    .iter()
                    .zip(predicate.trait_ref.args)
                    .all(|(&a, &b)| embeds(a, b, 0))
            {
                return Err("growing trait lifetime obligation cannot be proved");
            }
            parent = frame.parent;
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn enqueue_impl(
        &mut self,
        cx: &Cx<'_>,
        parent: Option<usize>,
        predicate: flux_ty::Predicate,
        imp: flux_hir::ImplId,
        lifetimes: Vec<Var>,
        args: Vec<Ty>,
        vars: Vec<Vec<Var>>,
        range: TextRange,
        depth: u32,
    ) {
        let regions = &cx.sigs.regions.impls[imp];
        let projections = cx.sigs.impl_projections.get(imp).map_or(&[][..], |p| p);
        let outer = Context::capture(cx);
        let mut bound = outer.clone();
        bound.projections = projections
            .iter()
            .copied()
            .zip(regions.projections.iter().cloned())
            .collect();
        let index = self.frames.len();
        self.frames.push(Frame {
            predicate,
            imp,
            size: goal_size(predicate),
            parent,
            lifetimes,
            args,
            vars,
            range,
            depth,
            outer,
            bound,
        });
        // A LIFO work stack retains the original order: each predicate and its
        // prerequisites finish before the next, then associated projections.
        for projection in (0..projections.len()).rev() {
            self.pending.push(Task::Projection {
                frame: index,
                index: projection,
            });
        }
        let predicates = cx.sigs.impl_predicates.get(imp).map_or(&[][..], |p| p);
        for predicate in (0..predicates.len()).rev() {
            self.pending.push(Task::Bound {
                frame: index,
                index: predicate,
            });
        }
    }
}

fn strict_subterm(needle: Ty, parent: Ty) -> bool {
    let mut seen = FxHashSet::default();
    let mut pending: Vec<_> = parent.children().collect();
    while let Some(ty) = pending.pop() {
        if ty == needle {
            return true;
        }
        if seen.insert(ty) {
            pending.extend(ty.children());
        }
    }
    false
}

/// A larger proof can introduce a different finite implementation path.
/// Reject growth when the old inputs remain embedded in the new inputs, rather
/// than using size alone (normalizing an associated value may increase size).
fn embeds(earlier: Ty, later: Ty, depth: u32) -> bool {
    fn visit(
        earlier: Ty,
        later: Ty,
        depth: u32,
        memo: &mut FxHashMap<(Ty, Ty, u32), bool>,
    ) -> bool {
        if earlier == later {
            return true;
        }
        if depth > EMBEDDING_DEPTH_LIMIT {
            return false;
        }
        let key = (earlier, later, depth);
        if let Some(&known) = memo.get(&key) {
            return known;
        }
        let nested = later
            .children()
            .any(|child| visit(earlier, child, depth + 1, memo));
        let same_head = match (earlier.kind(), later.kind()) {
            (TyKind::Adt(a, _), TyKind::Adt(b, _)) => a == b,
            (TyKind::Ref(a, _), TyKind::Ref(b, _))
            | (TyKind::RawPtr(a, _), TyKind::RawPtr(b, _)) => a == b,
            (TyKind::Tuple(a), TyKind::Tuple(b)) => a.len() == b.len(),
            (TyKind::Array(..), TyKind::Array(..)) | (TyKind::Slice(_), TyKind::Slice(_)) => true,
            _ => false,
        };
        let result = nested
            || same_head
                && earlier
                    .children()
                    .zip(later.children())
                    .all(|(a, b)| visit(a, b, depth + 1, memo));
        memo.insert(key, result);
        result
    }
    visit(earlier, later, depth, &mut FxHashMap::default())
}

fn goal_size(predicate: flux_ty::Predicate) -> usize {
    let mut seen = FxHashSet::default();
    let mut pending: Vec<_> = std::iter::once(predicate.self_ty)
        .chain(predicate.trait_ref.args.iter().copied())
        .collect();
    while let Some(ty) = pending.pop() {
        if seen.insert(ty) {
            pending.extend(ty.children());
        }
    }
    seen.len()
}

impl Cx<'_> {
    pub(super) fn prove(
        &mut self,
        predicate: flux_ty::Predicate,
        vars: &[Vec<Var>],
        lifetimes: &[Var],
        range: TextRange,
        depth: u32,
    ) {
        let root = OrdinaryGoal {
            predicate,
            vars: vars.to_vec(),
            lifetimes: lifetimes.to_vec(),
            range,
            depth,
        };
        let mut work = OrdinaryWork {
            pending: vec![Task::Goal(root, Context::capture(self), None)],
            frames: Vec::new(),
            proven: FxHashSet::default(),
        };
        while let Some(task) = work.pending.pop() {
            match task {
                Task::Goal(goal, context, parent) => {
                    let key = Key::new(&goal, &context);
                    if work.proven.contains(&key) {
                        continue;
                    }
                    let previous = context.install(self);
                    work.pending.push(Task::Complete(key, self.errors.len()));
                    self.prove_step(
                        goal.predicate,
                        &goal.vars,
                        &goal.lifetimes,
                        goal.range,
                        goal.depth,
                        parent,
                        &mut work,
                    );
                    previous.install(self);
                }
                Task::Bound { frame, index } => {
                    let data = &work.frames[frame];
                    let regions = &self.sigs.regions.impls[data.imp];
                    let predicates = self
                        .sigs
                        .impl_predicates
                        .get(data.imp)
                        .map_or(&[][..], |p| p);
                    let projections = self
                        .sigs
                        .impl_projections
                        .get(data.imp)
                        .map_or(&[][..], |projections| projections);
                    if !self.callable_outputs_complete(
                        &self.items.impls[data.imp].generics,
                        predicates,
                        projections,
                        data.range,
                    ) {
                        continue;
                    }
                    let previous = data.bound.clone().install(self);
                    let child = self.prepare_bound(
                        predicates[index],
                        regions.bound_selfs.get(index).map_or(&[][..], |row| row),
                        regions
                            .bound_traits
                            .get(index)
                            .unwrap_or(&TraitRefRegions::default()),
                        &data.lifetimes,
                        &data.args,
                        &data.vars,
                        data.range,
                        data.depth,
                    );
                    if let Some(child) = child {
                        work.pending
                            .push(Task::Goal(child, Context::capture(self), Some(frame)));
                    }
                    previous.install(self);
                }
                Task::Projection { frame, index } => {
                    let data = &work.frames[frame];
                    let regions = &self.sigs.regions.impls[data.imp];
                    let projections = self
                        .sigs
                        .impl_projections
                        .get(data.imp)
                        .map_or(&[][..], |p| p);
                    let previous = data.outer.clone().install(self);
                    let bounds: Vec<_> = self
                        .sigs
                        .impl_predicates
                        .get(data.imp)
                        .into_iter()
                        .flatten()
                        .copied()
                        .zip(regions.bound_selfs.iter())
                        .zip(regions.bound_traits.iter())
                        .map(|((predicate, own), rows)| (predicate, &**own, rows.clone()))
                        .collect();
                    let input = self.declared_projection_input(
                        &self.items.impls[data.imp].generics,
                        projections,
                        index,
                        &bounds,
                        data.range,
                    );
                    self.prove_projection(
                        projections[index],
                        &regions.projections[index],
                        input.as_ref(),
                        &data.lifetimes,
                        &data.args,
                        &data.vars,
                        data.range,
                    );
                    previous.install(self);
                }
                Task::Complete(key, before) => {
                    if self.errors.len() == before {
                        work.proven.insert(key);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_paths_reject_cycles_and_growth_but_allow_structural_descent() {
        let (_, _, hir) = flux_hir::lower_text(
            "ordinary-path.rs",
            "trait Mark<A>{}struct Wrap<T>(T);impl<T,A>Mark<A>for Wrap<T>{}fn main(){}",
        );
        let (sigs, errors) = flux_ty::signatures(&hir.items);
        assert!(errors.is_empty(), "{errors:?}");
        let (imp, data) = hir
            .items
            .impls
            .iter()
            .find(|(_, data)| {
                data.trait_
                    .is_some_and(|id| hir.items.traits[id].name.as_str() == "Mark")
            })
            .unwrap();
        let strukt = hir
            .items
            .structs
            .iter()
            .find(|(_, data)| data.name.as_str() == "Wrap")
            .unwrap()
            .0;
        let wrap = |ty| Ty::adt(strukt, hir.items.structs[strukt].name, &[ty]);
        let scalar = Ty::int(flux_ty::IntTy::U32);
        let predicate = |ty, arg| flux_ty::Predicate {
            self_ty: ty,
            trait_ref: flux_ty::TraitRef {
                trait_: data.trait_.unwrap(),
                name: hir.items.traits[data.trait_.unwrap()].name,
                args: Ty::list(&[arg]),
            },
        };
        let root = predicate(wrap(wrap(scalar)), scalar);
        let body = Body::empty(None);
        let requirements = Requirements::default();
        let cx = Cx::new(&body, &hir.items, &sigs, &requirements);
        let mut work = OrdinaryWork {
            pending: Vec::new(),
            frames: Vec::new(),
            proven: FxHashSet::default(),
        };
        work.enqueue_impl(
            &cx,
            None,
            root,
            imp,
            Vec::new(),
            vec![wrap(scalar), scalar],
            vec![Vec::new(), Vec::new()],
            TextRange::default(),
            0,
        );
        assert!(work.check_expansion(root, imp, Some(0)).is_err());
        assert!(
            work.check_expansion(predicate(wrap(wrap(wrap(scalar))), scalar), imp, Some(0))
                .is_err()
        );
        assert!(
            work.check_expansion(predicate(wrap(scalar), scalar), imp, Some(0))
                .is_ok()
        );
        let mut output = scalar;
        for _ in 0..16 {
            output = Ty::raw_ptr(flux_ty::Mutability::Not, output);
        }
        assert!(
            work.check_expansion(predicate(wrap(scalar), output), imp, Some(0))
                .is_ok()
        );
    }
}
