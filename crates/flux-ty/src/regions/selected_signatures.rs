//! Selected projection rows used only for checking an impl method's contract.
use super::*;

/// A comparison type with the row of its exact written occurrence.
pub struct SelectedSignatureType {
    pub ty: Ty,
    pub regions: Box<[Region]>,
}

/// Selected aliases retain their original inputs as separate proof obligations.
pub struct SelectedMethodSignature {
    pub trait_args: Box<[SelectedSignatureType]>,
    pub params: Box<[SelectedSignatureType]>,
    pub ret: SelectedSignatureType,
    pub selections: Box<[(crate::AliasTy, Box<[Region]>)]>,
}

/// Compare explicitly qualified legacy selections from their saved inputs.
/// Existing signature slots are copied; no input lifetime is replayed or invented.
pub fn selected_method_signature(
    items: &ItemTree,
    sigs: &Signatures,
    imp: flux_hir::ImplId,
    method: flux_hir::FunctionId,
) -> Option<SelectedMethodSignature> {
    let block = sigs.regions.impls.get(imp)?;
    let function = &items.functions[method];
    let signature = sigs.fns.get(method)?;
    let regions = sigs.regions.fns.get(method)?;
    let implemented = sigs.impl_traits.get(imp)?;
    if function.owner != Some(flux_hir::Owner::Impl(imp))
        || function.params.len() != signature.params.len()
        || signature.params.len() != regions.params.len()
        || implemented.args.len() != block.trait_args.len()
    {
        return None;
    }
    let mut selections = Vec::new();
    let written_args = items.impls[imp].trait_ref.as_ref()?.path.args(
        items.impls[imp]
            .trait_ref
            .as_ref()?
            .path
            .segments
            .len()
            .saturating_sub(1),
    );
    let mut trait_args = Vec::new();
    for (index, &ty) in implemented.args.iter().enumerate() {
        let row = block.trait_args.get(index)?;
        trait_args.push(match written_args.and_then(|args| args.types.get(index)) {
            Some(written) => select_occurrence(
                items,
                sigs,
                written,
                ty,
                row,
                block.header_selections.get(index + 1).map_or(&[], |s| s),
                &mut selections,
            )?,
            None => SelectedSignatureType {
                ty,
                regions: row.clone(),
            },
        });
    }
    if !regions.selected_inputs.is_empty()
        && regions.selected_inputs.len() != function.params.len() + 1
    {
        return None;
    }
    let params = function
        .params
        .iter()
        .zip(signature.params.iter())
        .zip(regions.params.iter())
        .enumerate()
        .map(|(index, ((written, &ty), row))| {
            select_occurrence(
                items,
                sigs,
                &written.ty,
                ty,
                row,
                regions.selected_inputs.get(index).map_or(&[], |s| s),
                &mut selections,
            )
        })
        .collect::<Option<Box<[_]>>>()?;
    let ret = select_occurrence(
        items,
        sigs,
        &function.ret,
        signature.ret,
        &regions.ret,
        regions
            .selected_inputs
            .get(function.params.len())
            .map_or(&[], |s| s),
        &mut selections,
    )?;
    Some(SelectedMethodSignature {
        trait_args: trait_args.into(),
        params,
        ret,
        selections: selections.into(),
    })
}

#[allow(clippy::too_many_arguments)]
fn select_occurrence(
    items: &ItemTree,
    sigs: &Signatures,
    written: &TypeRef,
    ty: Ty,
    regions: &[Region],
    inputs: &[ProjectionSelection],
    selections: &mut Vec<(crate::AliasTy, Box<[Region]>)>,
) -> Option<SelectedSignatureType> {
    let inputs: Vec<_> = inputs
        .iter()
        .filter(|input| input.qualified && !has_projection_inputs(input.alias))
        .cloned()
        .collect();
    let mut next = 0;
    let selected = select(
        items, sigs, written, ty, regions, &inputs, &mut next, selections,
    )?;
    (next == inputs.len()).then_some(selected)
}

#[allow(clippy::too_many_arguments)]
fn select(
    items: &ItemTree,
    sigs: &Signatures,
    written: &TypeRef,
    ty: Ty,
    regions: &[Region],
    inputs: &[ProjectionSelection],
    next: &mut usize,
    selections: &mut Vec<(crate::AliasTy, Box<[Region]>)>,
) -> Option<SelectedSignatureType> {
    if regions.len() != slots(items, ty) {
        return None;
    }
    if !ty.has_aliases() {
        return Some(SelectedSignatureType {
            ty,
            regions: regions.into(),
        });
    }
    if let (TypeRef::Path(path), TyKind::Alias(alias)) = (written, ty.kind()) {
        if path.qself.is_some() && !has_projection_inputs(alias) {
            let input = inputs.get(*next)?;
            if input.alias != alias
                || !input.lifetimes.is_empty()
                || input.args.len() != alias.args.len()
                || input
                    .args
                    .iter()
                    .zip(alias.args)
                    .any(|(row, &ty)| row.len() != slots(items, ty))
            {
                return None;
            }
            *next += 1;
            let input_row: Vec<_> = input.args.iter().flatten().copied().collect();
            let normalized = crate::traits::normalize(items, sigs, &[], ty);
            if normalized != ty {
                let row = selected::projection(items, sigs, alias, &input_row)?;
                if row.len() != slots(items, normalized) {
                    return None;
                }
                selections.push((alias, input_row.into()));
                return Some(SelectedSignatureType {
                    ty: normalized,
                    regions: row.into(),
                });
            }
            // An abstract output selects no concrete row. Preserve its exact
            // type/value row, and prove this saved occurrence's owning trait
            // from the original header when comparing the method contract.
            selections.push((alias, input_row.into()));
        }
        return Some(SelectedSignatureType {
            ty,
            regions: regions.into(),
        });
    }
    let (prefix, written_children): (usize, Vec<&TypeRef>) = match (written, ty.kind()) {
        (TypeRef::Ref { pointee, .. }, TyKind::Ref(..)) => (1, vec![pointee]),
        (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(..))
        | (TypeRef::Array { elem: pointee, .. }, TyKind::Array(..))
        | (TypeRef::Slice(pointee), TyKind::Slice(_)) => (0, vec![pointee]),
        (TypeRef::Tuple(elements), TyKind::Tuple(_)) => (0, elements.iter().collect()),
        (TypeRef::Path(path), TyKind::Adt(adt, _)) => (
            items.structs[adt.strukt].generics.lifetimes.len(),
            path.args(path.segments.len().saturating_sub(1))
                .into_iter()
                .flat_map(|args| args.types.iter())
                .collect(),
        ),
        _ => {
            return Some(SelectedSignatureType {
                ty,
                regions: regions.into(),
            });
        }
    };
    let mut result = regions.get(..prefix)?.to_vec();
    let mut at = prefix;
    let mut children = Vec::new();
    for (index, child) in ty.children().enumerate() {
        let count = slots(items, child);
        let row = regions.get(at..at + count)?;
        let selected = match written_children.get(index) {
            Some(written) => select(items, sigs, written, child, row, inputs, next, selections)?,
            None => SelectedSignatureType {
                ty: child,
                regions: row.into(),
            },
        };
        children.push(selected.ty);
        result.extend_from_slice(&selected.regions);
        at += count;
    }
    if at != regions.len() {
        return None;
    }
    let mut children = children.into_iter();
    let ty = ty.map_children(|_| children.next().expect("one entry per original child"));
    (result.len() == slots(items, ty)).then_some(SelectedSignatureType {
        ty,
        regions: result.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_method_inputs_reject_corruption_and_unused_occurrences() {
        let source = "trait Stream{type Checkpoint;}struct TokenSlice<'a,T>(&'a[T]);struct Checkpoint<T>(T);impl<'a,T>Stream for TokenSlice<'a,T>{type Checkpoint=Checkpoint<Self>;}trait Offset<S=Self>{fn offset(&self,other:&S)->usize;}impl<T>Offset<<TokenSlice<'_,T>as Stream>::Checkpoint>for TokenSlice<'_,T>{fn offset(&self,other:&<TokenSlice<'_,T>as Stream>::Checkpoint)->usize{0}}fn main(){}";
        for corruption in 0..4 {
            let (_, root, hir) = flux_hir::lower_text("selected_method_inputs.rs", source);
            let (mut sigs, diagnostics) = crate::signatures_checked(&hir.items, true, None);
            assert!(diagnostics.is_empty(), "{diagnostics:?}");
            let (imp, _) = hir
                .items
                .impls
                .iter()
                .find(|(id, data)| {
                    data.file == root
                        && sigs
                            .impl_traits
                            .get(*id)
                            .is_some_and(|t| t.name.as_str() == "Offset")
                })
                .unwrap();
            let (method, _) = hir
                .items
                .functions
                .iter()
                .find(|(_, data)| data.owner == Some(flux_hir::Owner::Impl(imp)))
                .unwrap();
            assert!(selected_method_signature(&hir.items, &sigs, imp, method).is_some());
            let value_rows = sigs.regions.fns[method].params.clone();
            match corruption {
                0 => {
                    let metadata = sigs.regions.fns.get_mut(method).unwrap();
                    metadata.selected_inputs[1][0].args[0] = vec![Region::Static; 2].into();
                }
                1 => {
                    let metadata = sigs.regions.fns.get_mut(method).unwrap();
                    let mut inputs = metadata.selected_inputs[1].to_vec();
                    inputs.push(inputs[0].clone());
                    metadata.selected_inputs[1] = inputs.into();
                }
                2 => {
                    let metadata = sigs.regions.impls.get_mut(imp).unwrap();
                    let mut inputs = metadata.header_selections[1].to_vec();
                    inputs.push(inputs[0].clone());
                    metadata.header_selections[1] = inputs.into();
                }
                3 => {
                    let metadata = sigs.regions.fns.get_mut(method).unwrap();
                    metadata.selected_inputs[1] = Box::default();
                }
                _ => unreachable!(),
            }
            assert!(selected_method_signature(&hir.items, &sigs, imp, method).is_none());
            assert_eq!(sigs.regions.fns[method].params, value_rows);
        }
    }
}
