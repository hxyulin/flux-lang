//! Region substitution for a concrete associated type selected at its written occurrence.
use super::*;

/// Normalize only occurrences whose written qualified Self can supply a row.
/// Family projections retain their existing captured-input representation.
pub(super) fn field_type(items: &ItemTree, sigs: &Signatures, written: &TypeRef, ty: Ty) -> Ty {
    match (written, ty.kind()) {
        (TypeRef::Path(path), TyKind::Alias(alias))
            if path.qself.is_some() && !has_projection_inputs(alias) =>
        {
            let output = flux_solve::traits::normalize(items, sigs, &[], ty);
            if output.has_aliases() { ty } else { output }
        }
        (TypeRef::Ptr { pointee, .. }, TyKind::RawPtr(..))
        | (TypeRef::Array { elem: pointee, .. }, TyKind::Array(..))
        | (TypeRef::Slice(pointee), TyKind::Slice(_)) => {
            ty.map_children(|child| field_type(items, sigs, pointee, child))
        }
        (TypeRef::Tuple(elements), TyKind::Tuple(_)) => {
            let mut elements = elements.iter();
            ty.map_children(|child| {
                elements
                    .next()
                    .map_or(child, |written| field_type(items, sigs, written, child))
            })
        }
        (TypeRef::Path(path), TyKind::Adt(..)) => {
            let mut arguments = path
                .args(path.segments.len().saturating_sub(1))
                .into_iter()
                .flat_map(|args| args.types.iter());
            ty.map_children(|child| {
                arguments
                    .next()
                    .map_or(child, |written| field_type(items, sigs, written, child))
            })
        }
        // An outer reference adds an output-outlives obligation on the
        // projection itself. Keep that original subject until declaration
        // checking can prove it, rather than infer it from the normalized RHS.
        _ => ty,
    }
}

pub(super) fn projection(
    items: &ItemTree,
    sigs: &Signatures,
    alias: crate::AliasTy,
    inputs: &[Region],
) -> Option<Vec<Region>> {
    let goal = crate::Predicate {
        self_ty: *alias.args.first()?,
        trait_ref: crate::TraitRef {
            trait_: alias.trait_,
            name: alias.trait_name,
            args: &alias.args[1..],
        },
    };
    let (imp, args) = flux_solve::traits::select_impl(items, sigs, goal)?;
    let metadata = sigs.regions.impls.get(imp)?;
    if !metadata.trait_lifetimes.is_empty()
        || !metadata.bounds.is_empty()
        || has_projection_inputs(alias)
        // A nested projection's zero-slot legacy type is not its input row.
        // It needs an independent occurrence mapping before header binding.
        || alias.args.iter().any(|ty| ty.has_aliases())
        || sigs
            .regions
            .traits
            .get(alias.trait_)
            .is_some_and(|r| !r.outlives.is_empty())
        || sigs.impl_predicates.get(imp).is_some_and(|predicates| {
            predicates.iter().any(|predicate| {
                let predicate = predicate.subst(&args);
                slots(items, predicate.self_ty) != 0
                    || predicate
                        .trait_ref
                        .args
                        .iter()
                        .any(|&ty| slots(items, ty) != 0)
                    || !items.traits[predicate.trait_ref.trait_]
                        .generics
                        .lifetimes
                        .is_empty()
            })
        })
    {
        return None;
    }
    let mut binding = Binding {
        items,
        args: &args,
        lifetimes: vec![None; metadata.names.len()],
        types: vec![None; args.len()],
    };
    let trait_args = sigs.impl_traits.get(imp)?.args;
    if metadata.trait_args.len() != trait_args.len() || alias.args.len() != trait_args.len() + 1 {
        return None;
    }
    let patterns = std::iter::once(sigs.impls[imp]).chain(trait_args.iter().copied());
    let tables = std::iter::once(&metadata.self_ty).chain(metadata.trait_args.iter());
    let mut at = 0;
    for ((pattern, table), &actual) in patterns.zip(tables).zip(alias.args) {
        let count = slots(items, actual);
        binding.bind(pattern, table, actual, inputs.get(at..at + count)?)?;
        at += count;
    }
    if at != inputs.len() {
        return None;
    }
    if args.iter().zip(&binding.types).any(|(&ty, row)| {
        row.as_ref()
            .map_or(slots(items, ty) != 0, |row| row.len() != slots(items, ty))
    }) {
        return None;
    }
    let template = flux_solve::traits::impl_type(sigs, imp, alias.name)?;
    // Unresolved child projections keep their original identity and slots.
    // A further selection that changes this RHS shape needs its own occurrence
    // mapping; never infer that row from a recursively normalized output.
    let output = template.subst(&args);
    if output != flux_solve::traits::normalize(items, sigs, &[], Ty::alias(alias)) {
        return None;
    }
    let own = &metadata
        .types
        .iter()
        .find(|(name, _)| *name == alias.name)?
        .1;
    let mut complete = true;
    let mut map = |region| match region {
        Region::Static => Region::Static,
        Region::Param(i) => binding
            .lifetimes
            .get(i as usize)
            .copied()
            .flatten()
            .unwrap_or_else(|| {
                complete = false;
                Region::Erased
            }),
        _ => {
            complete = false;
            Region::Erased
        }
    };
    let types = |index: u32| {
        binding
            .types
            .get(index as usize)
            .cloned()
            .flatten()
            .unwrap_or_else(|| vec![Region::Erased; slots(items, args[index as usize])])
    };
    let mut result = Vec::new();
    instantiate(items, template, own, &mut map, &types, &mut result);
    (complete && result.len() == slots(items, output)).then_some(result)
}

struct Binding<'a> {
    items: &'a ItemTree,
    args: &'a [Ty],
    lifetimes: Vec<Option<Region>>,
    types: Vec<Option<Vec<Region>>>,
}

impl Binding<'_> {
    fn bind(&mut self, pattern: Ty, own: &[Region], actual: Ty, row: &[Region]) -> Option<()> {
        if pattern.subst(self.args) != actual || row.len() != slots(self.items, actual) {
            return None;
        }
        if let TyKind::Param(param) = pattern.kind() {
            let slot = self.types.get_mut(param.index as usize)?;
            if slot.as_ref().is_some_and(|previous| previous != row) {
                return None;
            }
            *slot = Some(row.to_vec());
            return Some(());
        }
        let direct = match pattern.kind() {
            TyKind::Ref(..) | TyKind::Dynamic(_) => 1,
            TyKind::Adt(adt, _) => self.items.structs[adt.strukt].generics.lifetimes.len(),
            TyKind::Alias(_) | TyKind::Opaque(_) | TyKind::Closure(..) | TyKind::FnPtr(_) => {
                return (own.is_empty() && row.is_empty()).then_some(());
            }
            _ => 0,
        };
        for (&formal, &value) in own.get(..direct)?.iter().zip(row.get(..direct)?) {
            match formal {
                Region::Param(i) => {
                    let slot = self.lifetimes.get_mut(i as usize)?;
                    if slot.is_some_and(|previous| previous != value) {
                        return None;
                    }
                    *slot = Some(value);
                }
                Region::Static if value == Region::Static => {}
                _ => return None,
            }
        }
        let (mut p, mut a) = (direct, direct);
        for (pattern, actual) in pattern.children().zip(actual.children()) {
            let pn = slots(self.items, pattern);
            let an = slots(self.items, actual);
            self.bind(pattern, own.get(p..p + pn)?, actual, row.get(a..a + an)?)?;
            p += pn;
            a += an;
        }
        (p == own.len() && a == row.len()).then_some(())
    }
}
