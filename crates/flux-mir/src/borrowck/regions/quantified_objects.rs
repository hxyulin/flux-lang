//! Quantified principal/equality regions are instantiated independently per use.
use super::*;

pub(super) struct ObjectRows {
    pub owner: Vec<Var>,
    pub arguments: Vec<Vec<Var>>,
    pub associated: Vec<Vec<Var>>,
}
impl Cx<'_> {
    pub(super) fn quantified_object_rows(
        &mut self,
        object: flux_ty::DynTy,
        own: &[Var],
        bound: &[Var],
    ) -> Option<ObjectRows> {
        let signature = object.quantified_signature()?;
        let rows = flux_ty::regions::instantiate_fn_pointer_with(
            self.items,
            signature,
            own,
            bound,
            STATIC,
            |_, _| None,
        )?;
        let owners = self.items.traits[object.trait_].generics.lifetimes.len();
        if rows.len() != owners + object.args.len() + object.assoc_tys.len() + 1 {
            return None;
        }
        let owner = rows[..owners]
            .iter()
            .map(|row| match &**row {
                [var] => Some(*var),
                _ => None,
            })
            .collect::<Option<Vec<_>>>()?;
        let arguments = rows[owners..owners + object.args.len()].to_vec();
        let associated = rows[owners + object.args.len()..rows.len() - 1].to_vec();
        Some(ObjectRows {
            owner,
            arguments,
            associated,
        })
    }

    pub(super) fn quantified_object_output(
        &mut self,
        alias: AliasTy,
        owner: &[Var],
        inputs: &[Vec<Var>],
    ) -> Option<Vec<Var>> {
        let TyKind::Dynamic(object) = alias.args.first()?.kind() else {
            return None;
        };
        let signature = object.quantified_signature()?;
        let recipe = signature.regions?;
        if alias.trait_ != object.trait_
            || alias.args[1..] != *object.args
            || owner.len() != self.items.traits[object.trait_].generics.lifetimes.len()
        {
            return None;
        }
        let mut bound = vec![None; recipe.bound as usize];
        for (row, &var) in recipe.inputs_output.iter().zip(owner) {
            let [flux_ty::FnRegion::Bound(index)] = &**row else {
                return None;
            };
            let slot = bound.get_mut(*index as usize)?;
            if let Some(previous) = *slot
                && !self.projection_inputs_equal(&[previous], &[var])
            {
                return None;
            }
            *slot = Some(var);
        }
        let bound = bound.into_iter().collect::<Option<Vec<_>>>()?;
        let rows = self.quantified_object_rows(object, inputs.first()?.get(1..)?, &bound)?;
        if rows.arguments.len() + 1 != inputs.len()
            || rows
                .arguments
                .iter()
                .zip(&inputs[1..])
                .any(|(a, b)| !self.projection_inputs_equal(a, b))
        {
            return None;
        }
        let index = object
            .assoc_names
            .iter()
            .position(|name| *name == alias.name)?;
        rows.associated.get(index).cloned()
    }

    pub(super) fn unsize_quantified_object(
        &mut self,
        source: Ty,
        source_rows: &[Var],
        object: flux_ty::DynTy,
        object_rows: &[Var],
        category: Category,
        range: TextRange,
    ) {
        let Some(signature) = object.quantified_signature() else {
            return;
        };
        let recipe = signature.regions.expect("quantified object schema");
        let before = self.vars;
        let universals = self.universal.len();
        let known = self.known_pairs.len();
        let bound: Vec<_> = (0..recipe.bound)
            .map(|index| self.new_universal(format!("'object{index}")))
            .collect();
        self.close_known();
        let result = (|| {
            let rows = self.quantified_object_rows(object, object_rows.get(1..)?, &bound)?;
            let inputs: Vec<_> = std::iter::once(source_rows.to_vec())
                .chain(rows.arguments)
                .collect();
            self.prove(
                flux_solve::traits::dyn_principal(source, object),
                &inputs,
                &rows.owner,
                range,
                0,
            );
            for ((_, name), expected) in flux_solve::objects::dyn_assoc(self.items, object.trait_)
                .into_iter()
                .zip(rows.associated)
            {
                let alias = flux_solve::traits::alias_of(
                    self.items,
                    flux_ty::TraitRef {
                        trait_: object.trait_,
                        name: object.name,
                        args: object.args,
                    },
                    source,
                    name,
                );
                let values: Vec<_> = rows
                    .owner
                    .iter()
                    .copied()
                    .chain(inputs.iter().flatten().copied())
                    .collect();
                let regions: Vec<_> = (0..values.len())
                    .map(|index| Region::Param(index as u32))
                    .collect();
                let mut map = |region| match region {
                    Region::Param(index) => values.get(index as usize).copied(),
                    Region::Static => Some(STATIC),
                    _ => None,
                };
                let actual = self.instantiate_family(
                    alias,
                    &regions,
                    &mut 0,
                    &mut map,
                    &[],
                    &Vec::<Vec<Var>>::new(),
                );
                let actual = actual?;
                if actual.len() != expected.len() {
                    return None;
                }
                for (a, b) in actual.into_iter().zip(expected) {
                    self.equate(a, b, category, range);
                }
            }
            Some(())
        })();
        if result.is_none() {
            self.errors.push(Diagnostic::new(
                range,
                "quantified object region requirements cannot be proved",
            ));
        }
        self.finish_pointer_proof(before, &bound, &[], universals, known, range);
    }
}
