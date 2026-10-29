//! Declaration proofs use stored component rows, never conservative fresh rows.
use super::*;

impl Cx<'_> {
    pub(super) fn prove_auto_super(
        &mut self,
        goal: &ImplRegionGoal,
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<bool> {
        let requirements =
            flux_ty::traits::auto_trait_requirements(self.items, self.sigs, goal.predicate)?;
        Some(
            self.auto_super_goals(goal, &requirements, env, depth)
                .is_some_and(|goals| {
                    goals
                        .iter()
                        .all(|next| self.impl_prove_super(next, env, depth + 1))
                }),
        )
    }

    fn auto_super_goals(
        &mut self,
        goal: &ImplRegionGoal,
        requirements: &[flux_ty::Predicate],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<Vec<ImplRegionGoal>> {
        if !goal.lifetimes.is_empty() || !goal.bound.is_empty() || goal.vars.len() != 1 {
            return None;
        }
        let ty = goal.predicate.self_ty;
        let row = &goal.vars[0];
        if row.len() != self.slots(ty) as usize {
            return None;
        }
        if requirements.is_empty() {
            return Some(Vec::new());
        }
        if let TyKind::Adt(adt, args) = ty.kind() {
            let metadata = self.sigs.regions.structs.get(adt.strukt)?;
            let own = row.get(..metadata.lifetimes as usize)?;
            let mut at = own.len();
            let mut arg_rows = Vec::new();
            for &arg in args {
                let end = at + self.slots(arg) as usize;
                arg_rows.push(row.get(at..end)?.to_vec());
                at = end;
            }
            if at != row.len() {
                return None;
            }
            if flux_ty::is_lang_struct(self.items, adt.strukt, "phantom_data") {
                return self.auto_component_goals(requirements, args, &arg_rows, env, depth);
            }
            let fields = self.sigs.structs.get(adt.strukt)?;
            if fields.len() != requirements.len() || fields.len() != metadata.fields.len() {
                return None;
            }
            let tables = metadata.fields.clone();
            let fields = fields.to_vec();
            let mut goals = Vec::new();
            for ((field, table), requirement) in
                fields.into_iter().zip(tables.iter()).zip(requirements)
            {
                let next = self.impl_region_goal_with_proof(
                    flux_ty::Predicate {
                        self_ty: field,
                        trait_ref: requirement.trait_ref,
                    },
                    table,
                    &TraitRefRegions::default(),
                    own,
                    args,
                    &arg_rows,
                    Some((env, depth + 1)),
                    false,
                )?;
                if next.predicate.self_ty != self.normalize(requirement.self_ty) {
                    return None;
                }
                goals.push(next);
            }
            return Some(goals);
        }
        let components: Vec<Ty> = match ty.kind() {
            TyKind::Tuple(types) => types.to_vec(),
            TyKind::Array(element, _) | TyKind::Slice(element) => vec![element],
            TyKind::Ref(_, inner) | TyKind::RawPtr(_, inner) => vec![inner],
            TyKind::Closure(_, args) => vec![flux_ty::ClosureArgs::split(args).upvars],
            _ => return None,
        };
        let mut at = usize::from(matches!(ty.kind(), TyKind::Ref(..)));
        let mut rows = Vec::new();
        for &component in &components {
            let end = at + self.slots(component) as usize;
            rows.push(row.get(at..end)?.to_vec());
            at = end;
        }
        if at != row.len() {
            return None;
        }
        self.auto_component_goals(requirements, &components, &rows, env, depth)
    }

    fn auto_component_goals(
        &mut self,
        requirements: &[flux_ty::Predicate],
        components: &[Ty],
        rows: &[Vec<Var>],
        env: &[ImplRegionGoal],
        depth: u32,
    ) -> Option<Vec<ImplRegionGoal>> {
        if components.len() != requirements.len() || components.len() != rows.len() {
            return None;
        }
        let mut goals = Vec::new();
        for ((&component, row), requirement) in components.iter().zip(rows).zip(requirements) {
            // A changed projection needs its own occurrence input tables. ADT
            // fields use their declaration tables above; other containers may
            // carry an unchanged abstract header projection, but cannot invent
            // the output row of a newly selected family here.
            if component != requirement.self_ty || self.normalize(component) != component {
                return None;
            }
            let next = self.impl_region_goal_with_proof(
                flux_ty::Predicate {
                    self_ty: Ty::param(0, flux_intern::kw::SelfUpper),
                    trait_ref: requirement.trait_ref,
                },
                &[],
                &TraitRefRegions::default(),
                &[],
                &[component],
                std::slice::from_ref(row),
                Some((env, depth + 1)),
                false,
            )?;
            goals.push(next);
        }
        Some(goals)
    }

    pub(super) fn object_membership_outlives(&mut self, goal: &ImplRegionGoal) -> Option<()> {
        let declaration = self
            .sigs
            .regions
            .traits
            .get(goal.predicate.trait_ref.trait_)?;
        let requirements = declaration.outlives.clone();
        let args: Vec<_> = std::iter::once(goal.predicate.self_ty)
            .chain(goal.predicate.trait_ref.args.iter().copied())
            .collect();
        let map = |region| match region {
            Region::Static => Some(STATIC),
            Region::Param(index) => goal.lifetimes.get(index as usize).copied(),
            Region::Erased | Region::Late(_) => None,
        };
        for requirement in requirements {
            match requirement {
                Outlives::Region(a, b) => {
                    self.outlives(map(a)?, map(b)?, Category::Boring, TextRange::default())
                }
                Outlives::Type(ty, end) => {
                    let end = map(end)?;
                    let rows = self.instantiate_sig(ty, &[], &mut { map }, &args, &goal.vars)?;
                    for var in rows {
                        self.outlives(var, end, Category::Boring, TextRange::default());
                    }
                    self.type_outlives(self.normalize(ty.subst(&args)), end, TextRange::default());
                }
            }
        }
        Some(())
    }

    pub(super) fn object_super_goals(
        &mut self,
        goal: &ImplRegionGoal,
    ) -> Option<Vec<ImplRegionGoal>> {
        let TyKind::Dynamic(object) = goal.predicate.self_ty.kind() else {
            return None;
        };
        if flux_ty::fn_trait_kind(self.items, object.trait_).is_some()
            || !self.items.traits[object.trait_]
                .generics
                .lifetimes
                .is_empty()
        {
            return None;
        }
        let row = goal.vars.first()?;
        if row.len() != self.slots(goal.predicate.self_ty) as usize {
            return None;
        }
        let mut at = 1;
        let mut args = Vec::new();
        for (index, ty) in flux_ty::regions::object_region_types(self.items, object).enumerate() {
            let end = at + self.slots(ty) as usize;
            let own = row.get(at..end)?.to_vec();
            if index < object.args.len() {
                args.push(own);
            }
            at = end;
        }
        if at != row.len() || args.len() != object.args.len() {
            return None;
        }
        let root = ImplRegionGoal {
            predicate: flux_ty::traits::dyn_principal(goal.predicate.self_ty, object),
            vars: std::iter::once(row.clone()).chain(args).collect(),
            lifetimes: Vec::new(),
            bound: Vec::new(),
        };
        let mut goals = Vec::new();
        if !self.impl_super_goals(root, &mut goals, 0) {
            return None;
        }
        for &trait_ref in object.auto_traits {
            let marker = ImplRegionGoal {
                predicate: flux_ty::Predicate {
                    self_ty: goal.predicate.self_ty,
                    trait_ref,
                },
                vars: vec![row.clone()],
                lifetimes: Vec::new(),
                bound: Vec::new(),
            };
            if !self.impl_super_goals(marker, &mut goals, 0) {
                return None;
            }
        }
        Some(goals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn proof(source: &str) -> Option<bool> {
        let (_, _, hir) = flux_hir::lower_text("structural_proof.rs", source);
        assert!(hir.diagnostics.is_empty(), "{:?}", hir.diagnostics);
        let (sigs, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
        assert!(
            diagnostics.iter().all(|d| matches!(
                d.message.as_str(),
                "lifetime specialization and bounds in auto trait impls are not supported yet"
                    | "lifetime bound not satisfied"
            )),
            "{diagnostics:?}"
        );
        let imp = hir
            .items
            .impls
            .iter()
            .find(|(id, _)| {
                sigs.impl_traits
                    .get(*id)
                    .is_some_and(|t| t.name.as_str() == "Pass")
            })
            .unwrap()
            .0;
        impl_supertraits_match(&hir.items, &sigs, imp)
    }

    #[test]
    fn selected_static_impl_keeps_distinct_borrowed_field_rows() {
        for (bounds, expected) in [
            ("'a:'static,'b:'static", true),
            ("'a:'static,'b", false),
            ("'a,'b:'static", false),
        ] {
            let source = format!(
                "struct Guard<'a>(&'a u32);unsafe impl Send for Guard<'static>{{}}struct W<'a,'b>(Guard<'a>,Guard<'b>);trait Pass:Send{{}}impl<{bounds}>Pass for W<'a,'b>{{}}fn main(){{}}"
            );
            // This proof is tested below the unchanged public lifetime-marker
            // admission boundary. Both identical erased field types must retain
            // their own row when selecting the static-only implementation.
            assert_eq!(proof(&source), Some(expected), "{bounds}");
        }
    }

    #[test]
    fn recursive_static_impl_keeps_swapped_occurrence_rows() {
        for (bounds, expected) in [
            ("'a:'static,'b:'static", true),
            ("'a:'static,'b", false),
            ("'a,'b:'static", false),
        ] {
            let source = format!(
                "struct Guard<'a>(&'a u32);unsafe impl Send for Guard<'static>{{}}struct Node<'a,'b>{{value:Guard<'a>,other:&'b u32,next:Option<Box<Node<'b,'a>>>}}trait Pass:Send{{}}impl<{bounds}>Pass for Node<'a,'b>{{}}fn main(){{}}"
            );
            assert_eq!(proof(&source), Some(expected), "{bounds}");
        }
    }

    #[test]
    fn object_membership_does_not_assume_argument_outlives() {
        for (bound, expected) in [("", false), (":'static", true)] {
            let source = format!(
                "trait Foo<T>:std::any::Any{{}}trait Pass<T>:Foo<T>{{}}impl<'a{bound}>Pass<&'a str>for dyn Foo<&'a str>{{}}fn main(){{}}"
            );
            assert_eq!(proof(&source), Some(expected), "{bound}");
        }
    }
    #[test]
    fn object_principal_arguments_keep_independent_rows() {
        let (_, _, hir) = flux_hir::lower_text("object_rows.rs", "trait Get<A,B>{}fn main(){}");
        let (sigs, diagnostics) = flux_ty::signatures_checked(&hir.items, true, None);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let (trait_, data) = hir
            .items
            .traits
            .iter()
            .find(|(_, t)| t.name.as_str() == "Get")
            .unwrap();
        let args = Ty::list(&[
            Ty::new(TyKind::Ref(Mutability::Not, Ty::int(flux_ty::IntTy::U32))),
            Ty::new(TyKind::Ref(Mutability::Not, Ty::int(flux_ty::IntTy::U8))),
        ]);
        let object = flux_ty::DynTy {
            trait_,
            name: data.name,
            args,
            auto_traits: &[],
            assoc_names: &[],
            assoc_tys: &[],
        };
        let body = Body::empty(None);
        let none = Requirements::default();
        for (first_static, second_static, expected) in [
            (true, true, true),
            (true, false, false),
            (false, true, false),
            (false, false, false),
        ] {
            let mut cx = Cx::new(&body, &hir.items, &sigs, &none);
            assert_eq!(cx.new_universal("'static".into()), STATIC);
            let a = cx.new_universal("'a".into());
            let b = cx.new_universal("'b".into());
            if first_static {
                cx.known_pairs.push((a, STATIC));
            }
            if second_static {
                cx.known_pairs.push((b, STATIC));
            }
            let goal = ImplRegionGoal {
                predicate: flux_ty::traits::dyn_principal(Ty::new(TyKind::Dynamic(object)), object),
                vars: vec![vec![STATIC, a, b], vec![STATIC], vec![STATIC]],
                lifetimes: Vec::new(),
                bound: Vec::new(),
            };
            assert!(cx.impl_prove_super(&goal, &[], 0));
            cx.close_known();
            let solution = cx.solve(&Points::new(&body), &[], &[]);
            assert_eq!(
                cx.check_universal(&solution).is_empty(),
                expected,
                "a_static={first_static}, b_static={second_static}"
            );
        }
    }
}
