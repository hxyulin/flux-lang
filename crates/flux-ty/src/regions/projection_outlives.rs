//! Recover associated output occurrences from existing signature region tables.
use super::*;

/// An implied output-outlives requirement with its original selection inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionOutlives {
    pub ty: Ty,
    pub regions: Box<[Region]>,
    pub target: Region,
}

/// Recover reference-implied projection requirements without changing storage.
/// A projection's inputs identify the occurrence; they are not stored output
/// lifetimes and must not themselves be required to outlive the reference.
pub fn type_projection_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    ty: Ty,
    row: &[Region],
) -> Vec<ProjectionOutlives> {
    let mut walk = ProjectionWalk {
        items,
        sigs,
        active: Vec::new(),
        projected: Default::default(),
        visited: Default::default(),
        complete: Default::default(),
        out: Vec::new(),
    };
    if walk.visit(ty, row, &[]).is_none() {
        walk.out.clear();
    }
    walk.out
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Occurrence {
    ty: Ty,
    row: Box<[Region]>,
    bounds: Box<[Region]>,
}

struct ProjectionWalk<'a> {
    items: &'a ItemTree,
    sigs: &'a Signatures,
    active: Vec<flux_hir::StructId>,
    projected: rustc_hash::FxHashMap<flux_hir::StructId, bool>,
    visited: rustc_hash::FxHashSet<(Occurrence, Vec<flux_hir::StructId>)>,
    complete: rustc_hash::FxHashSet<Occurrence>,
    out: Vec<ProjectionOutlives>,
}

impl ProjectionWalk<'_> {
    /// Returns the earliest ancestor that blocked a field expansion. An
    /// occurrence with no external cutoff can be reused on every path.
    fn visit(&mut self, ty: Ty, row: &[Region], bounds: &[Region]) -> Option<usize> {
        let key = Occurrence {
            ty,
            row: row.into(),
            bounds: bounds.into(),
        };
        if self.complete.contains(&key) {
            return Some(usize::MAX);
        }
        if !self.visited.insert((key.clone(), self.active.clone())) {
            return Some(if self.active.is_empty() {
                usize::MAX
            } else {
                0
            });
        }
        let blocked = self.walk(ty, row, bounds)?;
        if blocked == usize::MAX {
            self.complete.insert(key);
        }
        Some(blocked)
    }

    fn walk(&mut self, ty: Ty, row: &[Region], bounds: &[Region]) -> Option<usize> {
        let items = self.items;
        let sigs = self.sigs;
        if row.len() != slots(items, ty) {
            return None;
        }
        if let TyKind::Ref(_, inner) = ty.kind() {
            let &region = row.first()?;
            if bounds.contains(&region) {
                return self.visit(inner, row.get(1..)?, bounds);
            }
            let mut bounds = bounds.to_vec();
            bounds.push(region);
            return self.visit(inner, row.get(1..)?, &bounds);
        }
        let mut blocked = usize::MAX;
        if let TyKind::Alias(alias) = ty.kind() {
            if has_projection_inputs(items, alias) {
                for &target in bounds {
                    let requirement = ProjectionOutlives {
                        ty,
                        regions: row.into(),
                        target,
                    };
                    if !self.out.contains(&requirement) {
                        self.out.push(requirement);
                    }
                }
                let mut at = alias.owner_lifetimes as usize;
                for &arg in alias.args {
                    let end = at.checked_add(slots(items, arg))?;
                    blocked = blocked.min(self.visit(arg, row.get(at..end)?, &[])?);
                    at = end;
                }
                at += alias.family_lifetimes as usize;
                for &arg in alias.family_args {
                    let end = at.checked_add(slots(items, arg))?;
                    blocked = blocked.min(self.visit(arg, row.get(at..end)?, &[])?);
                    at = end;
                }
                if at != row.len() {
                    return None;
                }
            }
            return Some(blocked);
        }
        let (children, mut at): (Vec<Ty>, usize) = match ty.kind() {
            TyKind::Adt(adt, args) => (
                args.to_vec(),
                items.structs[adt.strukt].generics.lifetimes.len(),
            ),
            TyKind::Dynamic(object) => (
                object_region_types(items, object).collect(),
                object_own_slots(items, object),
            ),
            TyKind::Closure(_, args) => (vec![ClosureArgs::split(args).upvars], 0),
            TyKind::FnPtr(_) | TyKind::FnDef(_) => return Some(blocked),
            TyKind::Array(inner, _) => (vec![inner], 0),
            _ => (ty.children().collect(), 0),
        };
        for child in children {
            let end = at.checked_add(slots(items, child))?;
            blocked = blocked.min(self.visit(child, row.get(at..end)?, bounds)?);
            at = end;
        }
        // Nominal fields can store a projection absent from the type arguments.
        // Reconstruct each field's original row before substituting its types.
        let TyKind::Adt(adt, args) = ty.kind() else {
            return Some(blocked);
        };
        if !self.has_projected_fields(adt.strukt, ty) {
            return Some(blocked);
        }
        if let Some(index) = self.active.iter().position(|&id| id == adt.strukt) {
            return Some(blocked.min(index));
        }
        let metadata = sigs.regions.structs.get(adt.strukt)?;
        let mut at = metadata.lifetimes as usize;
        let mut arguments = Vec::new();
        for &arg in args {
            let end = at.checked_add(slots(items, arg))?;
            arguments.push(row.get(at..end)?.to_vec());
            at = end;
        }
        let depth = self.active.len();
        self.active.push(adt.strukt);
        for (&field, table) in sigs
            .structs
            .get(adt.strukt)?
            .iter()
            .zip(metadata.fields.iter())
        {
            let mut field_row = Vec::new();
            instantiate(
                items,
                field,
                table,
                &mut |region| match region {
                    Region::Param(index) => {
                        row.get(index as usize).copied().unwrap_or(Region::Erased)
                    }
                    region => region,
                },
                &|index| arguments.get(index as usize).cloned().unwrap_or_default(),
                &mut field_row,
            );
            let field_blocked = self.visit(field.subst(args), &field_row, bounds)?;
            if field_blocked < depth {
                blocked = blocked.min(field_blocked);
            }
        }
        self.active.pop();
        Some(blocked)
    }

    fn has_projected_fields(&mut self, id: flux_hir::StructId, ty: Ty) -> bool {
        if let Some(&projected) = self.projected.get(&id) {
            return projected;
        }
        // The graph follows declaration fields. Arguments are visited
        // separately and do not change whether this declaration expands.
        let projected = outlives_structs_with_projections(self.sigs, ty);
        let found = projected.contains(&id);
        self.projected
            .extend(projected.into_iter().map(|id| (id, true)));
        self.projected.insert(id, found);
        found
    }
}

/// Recover reference-implied projections in every parameter and return type.
pub fn signature_projection_outlives(
    items: &ItemTree,
    sigs: &Signatures,
    function: flux_hir::FunctionId,
) -> Vec<ProjectionOutlives> {
    let Some(regions) = sigs.regions.fns.get(function) else {
        return Vec::new();
    };
    let signature = &sigs.fns[function];
    signature
        .params
        .iter()
        .chain([&signature.ret])
        .zip(regions.params.iter().chain([&regions.ret]))
        .flat_map(|(&ty, row)| type_projection_outlives(items, sigs, ty, row))
        .collect()
}
