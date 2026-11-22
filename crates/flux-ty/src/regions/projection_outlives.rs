//! Recover associated output occurrences from existing signature region tables.
use super::*;
use std::cell::RefCell;

const MAX_CACHED_ROWS: usize = 16_384;

type ProjectionRows = rustc_hash::FxHashMap<Box<[Region]>, Vec<ProjectionOutlives>>;

struct Cache {
    items: *const ItemTree,
    sigs: *const Signatures,
    rows: rustc_hash::FxHashMap<Ty, ProjectionRows>,
    entries: usize,
}

impl Cache {
    fn matches(&self, items: &ItemTree, sigs: &Signatures) -> bool {
        std::ptr::eq(self.items, items) && std::ptr::eq(self.sigs, sigs)
    }
}

thread_local! {
    static CACHE: RefCell<Option<Cache>> = const { RefCell::new(None) };
}

/// Reuses completed walks while the item tree and signatures are immutable.
pub fn with_projection_outlives_cache<T>(
    items: &ItemTree,
    sigs: &Signatures,
    run: impl FnOnce() -> T,
) -> T {
    let previous = CACHE.with_borrow_mut(|cache| {
        cache.replace(Cache {
            items,
            sigs,
            rows: Default::default(),
            entries: 0,
        })
    });
    let _restore = RestoreCache(previous);
    run()
}

struct RestoreCache(Option<Cache>);

impl Drop for RestoreCache {
    fn drop(&mut self) {
        CACHE.with_borrow_mut(|cache| *cache = self.0.take());
    }
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
    let cached = CACHE.with_borrow(|cache| {
        cache
            .as_ref()
            .filter(|cache| cache.matches(items, sigs))?
            .rows
            .get(&ty)?
            .get(row)
            .cloned()
    });
    if let Some(cached) = cached {
        return cached;
    }
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
    CACHE.with_borrow_mut(|cache| {
        let Some(cache) = cache.as_mut().filter(|cache| cache.matches(items, sigs)) else {
            return;
        };
        if cache.entries >= MAX_CACHED_ROWS {
            cache.rows.clear();
            cache.entries = 0;
        }
        cache
            .rows
            .entry(ty)
            .or_default()
            .insert(row.into(), walk.out.clone());
        cache.entries += 1;
    });
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
        let expands_fields = self.has_projected_fields(adt.strukt, ty);
        let Some(metadata) = sigs.regions.structs.get(adt.strukt) else {
            return (!expands_fields).then_some(blocked);
        };
        let has_bounds = metadata
            .outlives
            .iter()
            .any(|bound| matches!(bound, Outlives::Type(..)));
        if !expands_fields && !has_bounds {
            return Some(blocked);
        }
        let mut at = metadata.lifetimes as usize;
        let mut arguments = Vec::new();
        for &arg in args {
            let end = at.checked_add(slots(items, arg))?;
            arguments.push(row.get(at..end)?.to_vec());
            at = end;
        }
        // A nominal requirement can name a projection of a type parameter,
        // for example `M::Data: 'static`. Its erased template may have no
        // slots, while its substituted owner has lifetime arguments.
        for &bound in &metadata.outlives {
            let Outlives::Type(subject, target) = bound else {
                continue;
            };
            if slots(items, subject) != 0 {
                continue;
            }
            let instance = subject.subst(args);
            let TyKind::Alias(alias) = instance.kind() else {
                continue;
            };
            if !has_projection_inputs(items, alias) {
                continue;
            }
            let target = match target {
                Region::Static => Region::Static,
                Region::Param(index) if index < metadata.lifetimes => *row.get(index as usize)?,
                _ => continue,
            };
            let mut selected = Vec::new();
            instantiate(
                items,
                subject,
                &[],
                &mut |_| Region::Erased,
                &|index| arguments.get(index as usize).cloned().unwrap_or_default(),
                &mut selected,
            );
            if selected.len() == slots(items, instance) && !selected.contains(&Region::Erased) {
                let fact = ProjectionOutlives {
                    ty: instance,
                    regions: selected.into(),
                    target,
                };
                if !self.out.contains(&fact) {
                    self.out.push(fact);
                }
            }
        }
        if !expands_fields {
            return Some(blocked);
        }
        if let Some(index) = self.active.iter().position(|&id| id == adt.strukt) {
            return Some(blocked.min(index));
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
    let mut out: Vec<_> = signature
        .params
        .iter()
        .chain([&signature.ret])
        .zip(regions.params.iter().chain([&regions.ret]))
        .flat_map(|(&ty, row)| type_projection_outlives(items, sigs, ty, row))
        .collect();
    if let Some(imp) = items.functions[function]
        .owner
        .and_then(flux_hir::Owner::as_impl)
        && let Some(owner) = sigs.regions.impls.get(imp)
    {
        // Method summaries include requirements implied by the impl header,
        // even for associated functions with no Self parameter.
        out.extend(type_projection_outlives(
            items,
            sigs,
            sigs.impls[imp],
            &owner.self_ty,
        ));
        if let Some(trait_ref) = sigs.impl_traits.get(imp) {
            for (&ty, row) in trait_ref.args.iter().zip(&owner.trait_args) {
                out.extend(type_projection_outlives(items, sigs, ty, row));
            }
        }
    }
    out
}
