//! Method values use the same selected types and source rows as their proof.
use super::*;

impl Cx<'_> {
    /// Cache only within this region check. The selected row keeps the original
    /// function namespace and never becomes a global fact about an erased Ty.
    pub(super) fn selected_source_signature(
        &mut self,
        function: flux_hir::FunctionId,
        range: TextRange,
    ) -> Result<Option<std::rc::Rc<flux_ty::regions::SelectedMethodSignature>>, ()> {
        let Some(flux_hir::Owner::Impl(imp)) = self.items.functions[function].owner else {
            return Ok(None);
        };
        if self.sigs.impl_traits.get(imp).is_none() {
            return Ok(None);
        }
        // The selected-signature walker copies alias-free value rows verbatim.
        // Those methods need neither a query nor stricter value-row handling.
        let original = &self.sigs.fns[function];
        if original
            .params
            .iter()
            .chain([&original.ret])
            .all(|ty| !ty.has_aliases())
        {
            return Ok(None);
        }
        let signature = self
            .selected_source_signatures
            .entry(function)
            .or_insert_with(|| {
                flux_ty::regions::selected_method_signature(self.items, self.sigs, imp, function)
                    .map(std::rc::Rc::new)
            })
            .clone();
        match signature {
            Some(signature) => {
                let original = &self.sigs.fns[function];
                let regions = &self.sigs.regions.fns[function];
                let changed = signature
                    .params
                    .iter()
                    .zip(original.params.iter())
                    .zip(regions.params.iter())
                    .any(|((value, &ty), row)| value.ty != ty || value.regions != *row)
                    || signature.ret.ty != original.ret
                    || signature.ret.regions != regions.ret;
                Ok(changed.then_some(signature))
            }
            None => {
                self.errors.push(Diagnostic::new(
                    range,
                    "selected method signature regions are not represented",
                ));
                Err(())
            }
        }
    }

    pub(super) fn selected_value_shape(
        &mut self,
        ty: Ty,
        vars: Option<Vec<Var>>,
        actual: Ty,
        range: TextRange,
    ) -> Option<Vec<Var>> {
        let expected = self.normalize(ty);
        let actual = self.normalize(actual);
        let vars =
            vars.filter(|vars| expected == actual && vars.len() == self.slots(actual) as usize);
        if vars.is_none() {
            self.errors.push(Diagnostic::new(
                range,
                "selected method value region rows do not match",
            ));
        }
        vars
    }
    pub(super) fn selected_local_shape(
        &mut self,
        ty: Ty,
        vars: Option<Vec<Var>>,
        local: Local,
        range: TextRange,
    ) -> Option<Vec<Var>> {
        let vars = self.selected_value_shape(ty, vars, self.body.locals[local].ty, range)?;
        if vars.len() != self.local_vars(local).len() {
            self.errors.push(Diagnostic::new(
                range,
                "selected method local region rows do not match",
            ));
            return None;
        }
        Some(vars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_selected_return_rows_diagnose_without_fresh_universals() {
        let source = "trait S{type Item;}struct W<'a,T>(&'a u32,T);impl<'a,T:S>S for W<'a,T>{type Item=(&'a u32,T::Item);}trait M<A>{fn f(&self)->A;}impl<'a,T:S<Item=u32>>M<<W<'a,T>as S>::Item>for W<'a,T>{fn f(&self)-><W<'a,T>as S>::Item{(self.0,0)}}fn main(){}";
        for corruption in 0..3 {
            let (_, root, hir) = flux_hir::lower_text("selected_body_rows.rs", source);
            let mut typed = crate::check_file(&hir);
            assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
            let mir = crate::build_file(&hir, &typed);
            assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
            let (imp, _) = hir
                .items
                .impls
                .iter()
                .find(|(id, data)| {
                    data.file == root
                        && typed
                            .sigs
                            .impl_traits
                            .get(*id)
                            .is_some_and(|t| t.name.as_str() == "M")
                })
                .unwrap();
            let (method, _) = hir
                .items
                .functions
                .iter()
                .find(|(_, data)| data.owner == Some(flux_hir::Owner::Impl(imp)))
                .unwrap();
            let mut body = mir.bodies[method].clone();
            let metadata = typed.sigs.regions.fns.get_mut(method).unwrap();
            let names = metadata.names.len();
            let ret = metadata.params.len();
            match corruption {
                0 => metadata.selected_inputs[ret] = Box::default(),
                1 => metadata.selected_inputs[ret][0].args[0] = Box::default(),
                2 => body.locals[Body::RETURN].ty = Ty::UNIT,
                _ => unreachable!(),
            }
            let requirements = Requirements::default();
            let mut cx = Cx::new(&body, &hir.items, &typed.sigs, &requirements);
            cx.universal_regions();
            assert!(
                cx.errors
                    .iter()
                    .any(|error| error.message.starts_with("selected method")),
                "{:?}",
                cx.errors
            );
            assert_eq!(
                cx.universal.len(),
                names + 1,
                "no unconstrained replacement row"
            );
        }
    }
}
