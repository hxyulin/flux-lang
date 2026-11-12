//! Structural checks performed before imported MIR reaches compiler queries.

use flux_hir::ItemTree;
use flux_hir::flib::Error;
use flux_ty::{ClosureArgs, Signatures, Ty, TyKind};

use crate::eval::{StaticProjection, Value};
use crate::*;

fn invalid() -> Error {
    Error::new("invalid imported MIR structure")
}

fn fields(ty: Ty, variant: Option<u32>, sigs: &Signatures) -> Result<Vec<Ty>, Error> {
    match ty.kind() {
        TyKind::Tuple(fields) => Ok(fields.to_vec()),
        TyKind::Closure(_, args) => {
            if args.len() < 3 {
                return Err(invalid());
            }
            match ClosureArgs::split(args).upvars.kind() {
                TyKind::Tuple(fields) => Ok(fields.to_vec()),
                _ => Err(invalid()),
            }
        }
        TyKind::Adt(adt, args) => {
            let all = sigs.structs.get(adt.strukt).ok_or_else(invalid)?;
            let selected = if let Some(starts) = sigs.variant_starts.get(adt.strukt) {
                let variant = variant.unwrap_or(0) as usize;
                let start = *starts.get(variant).ok_or_else(invalid)? as usize;
                let end = *starts
                    .get(variant.checked_add(1).ok_or_else(invalid)?)
                    .ok_or_else(invalid)? as usize;
                all.get(start..end).ok_or_else(invalid)?
            } else {
                all
            };
            Ok(selected.iter().map(|ty| ty.subst(args)).collect())
        }
        _ => Err(invalid()),
    }
}

fn variant(items: &ItemTree, strukt: flux_hir::StructId, index: u32) -> Result<(), Error> {
    items
        .structs
        .get(strukt)
        .and_then(|s| s.variants.as_ref())
        .and_then(|v| v.get(index as usize))
        .map(|_| ())
        .ok_or_else(invalid)
}

fn ty(ty: Ty, items: &ItemTree, sigs: &Signatures, depth: u32) -> Result<(), Error> {
    ty_at(
        ty,
        items,
        sigs,
        depth,
        &mut rustc_hash::FxHashSet::default(),
    )
}

fn ty_at(
    ty: Ty,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
    validated: &mut rustc_hash::FxHashSet<(Ty, u32)>,
) -> Result<(), Error> {
    if depth > 128 {
        return Err(invalid());
    }
    if validated.contains(&(ty, depth)) {
        return Ok(());
    }
    match ty.kind() {
        TyKind::Adt(adt, _) => {
            items.structs.get(adt.strukt).ok_or_else(invalid)?;
            sigs.structs.get(adt.strukt).ok_or_else(invalid)?;
        }
        TyKind::FnDef(f) => {
            items.functions.get(f.function).ok_or_else(invalid)?;
            sigs.fns.get(f.function).ok_or_else(invalid)?;
        }
        TyKind::Dynamic(d) => {
            let principal = items.traits.get(d.trait_).ok_or_else(invalid)?;
            if !d.valid_quantified(principal.generics.lifetimes.len()) {
                return Err(invalid());
            }
            sigs.traits.get(d.trait_).ok_or_else(invalid)?;
        }
        TyKind::Alias(a) => {
            items.traits.get(a.trait_).ok_or_else(invalid)?;
            if a.args.is_empty() {
                return Err(invalid());
            }
        }
        TyKind::Opaque(o) => {
            items.functions.get(o.function).ok_or_else(invalid)?;
            if sigs
                .opaques
                .get(o.function)
                .and_then(|v| v.get(o.index as usize))
                .is_none()
            {
                return Err(invalid());
            }
        }
        TyKind::Closure(_, args) => {
            if args.len() < 3 {
                return Err(invalid());
            }
            let c = ClosureArgs::split(args);
            if c.closure_kind().is_none()
                || !matches!(c.sig.kind(),TyKind::Tuple([params,_]) if matches!(params.kind(),TyKind::Tuple(_)))
                || !matches!(c.upvars.kind(), TyKind::Tuple(_))
            {
                return Err(invalid());
            }
        }
        TyKind::FnPtr(f) => {
            if f.inputs_output.is_empty() {
                return Err(invalid());
            }
        }
        TyKind::Error | TyKind::Infer(_) => return Err(invalid()),
        _ => {}
    }
    for child in ty.children() {
        ty_at(child, items, sigs, depth + 1, validated)?;
    }
    validated.insert((ty, depth));
    Ok(())
}

fn place(p: &Place, body: &Body, items: &ItemTree, sigs: &Signatures) -> Result<Ty, Error> {
    let mut ty = body.locals.get(p.local).ok_or_else(invalid)?.ty;
    let mut selected = None;
    for elem in &p.projection {
        if ty.has_aliases() {
            // Source MIR may index a selected associated array without
            // rewriting the containing field's declaration. Decode it using
            // the same owner equalities the source checker used.
            let env = body.owner.map_or_else(Vec::new, |f| {
                flux_solve::traits::projections_of(items, sigs, f)
            });
            ty = flux_solve::traits::normalize(items, sigs, &env, ty);
        }
        match *elem {
            ProjectionElem::Downcast(index) => {
                let TyKind::Adt(adt, _) = ty.kind() else {
                    return Err(invalid());
                };
                variant(items, adt.strukt, index)?;
                selected = Some(index);
                continue;
            }
            ProjectionElem::Field(index) => {
                ty = *fields(ty, selected, sigs)?
                    .get(index as usize)
                    .ok_or_else(invalid)?;
            }
            ProjectionElem::Deref => {
                ty = match ty.kind() {
                    TyKind::Ref(_, p) | TyKind::RawPtr(_, p) => p,
                    _ => return Err(invalid()),
                };
            }
            ProjectionElem::BoxDeref => {
                ty = flux_solve::box_pointee(ty).ok_or_else(invalid)?;
            }
            ProjectionElem::Index(index) => {
                let index_ty = body.locals.get(index).ok_or_else(invalid)?.ty;
                if index_ty != Ty::int(flux_ty::IntTy::Usize) {
                    return Err(invalid());
                }
                ty = ty.sequence_elem().ok_or_else(invalid)?;
            }
            ProjectionElem::ConstantIndex {
                offset,
                min_length,
                from_end,
            } => {
                let length = ty.array_len();
                if (from_end && (offset == 0 || offset > min_length))
                    || (!from_end && offset >= min_length)
                    || length.is_some_and(|n| n < u64::from(min_length))
                {
                    return Err(invalid());
                }
                ty = ty.sequence_elem().ok_or_else(invalid)?;
            }
            ProjectionElem::Subslice { from, to, from_end } => {
                let element = ty.sequence_elem().ok_or_else(invalid)?;
                if let Some(length) = ty.array_len() {
                    let end = if from_end {
                        length.checked_sub(u64::from(to)).ok_or_else(invalid)?
                    } else {
                        u64::from(to)
                    };
                    if u64::from(from) > end || end > length {
                        return Err(invalid());
                    }
                    ty = Ty::array(element, end - u64::from(from));
                } else if !from_end && from > to {
                    return Err(invalid());
                }
            }
        }
        selected = None;
    }
    Ok(ty)
}

fn constant(c: &Constant, body: &Body, items: &ItemTree, sigs: &Signatures) -> Result<(), Error> {
    match c {
        Constant::Fn { function, ty, .. } => {
            items.functions.get(*function).ok_or_else(invalid)?;
            sigs.fns.get(*function).ok_or_else(invalid)?;
            self::ty(*ty, items, sigs, 0)?;
        }
        Constant::Item { id, ty, .. } | Constant::Static { id, ty } => {
            items.consts.get(*id).ok_or_else(invalid)?;
            sigs.consts.get(*id).ok_or_else(invalid)?;
            self::ty(*ty, items, sigs, 0)?;
        }
        Constant::ItemRef { id, args, ty } => {
            let declaration = items.consts.get(*id).ok_or_else(invalid)?;
            let value_ty = *sigs.consts.get(*id).ok_or_else(invalid)?;
            self::ty(*ty, items, sigs, 0)?;
            for &arg in *args {
                self::ty(arg, items, sigs, 0)?;
            }
            let TyKind::Ref(Mutability::Not, pointee) = ty.kind() else {
                return Err(invalid());
            };
            let env = body.owner.map_or_else(Vec::new, |f| {
                flux_solve::traits::projections_of(items, sigs, f)
            });
            let bounds = body.owner.map_or_else(Vec::new, |f| {
                flux_solve::traits::predicates_of(items, sigs, f)
            });
            let value_ty = flux_solve::traits::normalize(items, sigs, &env, value_ty.subst(args));
            if declaration.is_static.is_some() || value_ty != pointee {
                return Err(Error::new("invalid imported constant reference type"));
            }
            if flux_solve::traits::needs_drop(items, sigs, &bounds, pointee)
                || flux_solve::traits::has_interior_mutability(items, sigs, pointee)
            {
                return Err(Error::new(
                    "invalid imported constant reference qualification",
                ));
            }
        }
        Constant::FnItem(ty) => {
            self::ty(*ty, items, sigs, 0)?;
            if !matches!(ty.kind(), TyKind::FnDef(_)) {
                return Err(invalid());
            }
        }
        Constant::Promoted { index, ty } => {
            self::ty(*ty, items, sigs, 0)?;
            let TyKind::Ref(_, pointee) = ty.kind() else {
                return Err(invalid());
            };
            let v = body.promoted.get(*index as usize).ok_or_else(invalid)?;
            value_for(v, pointee, items, sigs, 0)?;
        }
        Constant::Inline { index, ty } => {
            self::ty(*ty, items, sigs, 0)?;
            let constant = body
                .inline_constants
                .get(*index as usize)
                .ok_or_else(invalid)?;
            let stored = constant.locals.get(Body::RETURN).ok_or_else(invalid)?.ty;
            if stored != *ty {
                return Err(invalid());
            }
        }
        Constant::TyConst { value, ty } => {
            self::ty(*value, items, sigs, 0)?;
            self::ty(*ty, items, sigs, 0)?;
        }
        Constant::CStr(i) => {
            body.c_strings.get(*i as usize).ok_or_else(invalid)?;
        }
        _ => {}
    }
    Ok(())
}
fn operand(o: &Operand, body: &Body, items: &ItemTree, sigs: &Signatures) -> Result<(), Error> {
    match o {
        Operand::Copy(p) | Operand::Move(p) => {
            place(p, body, items, sigs)?;
        }
        Operand::Const(c) => constant(c, body, items, sigs)?,
    };
    Ok(())
}

/// Explicit early function lifetime rows name caller universals. They cannot
/// manufacture a binder or attach a row to an unrelated call site.
fn named_call_regions(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    names: usize,
) -> Result<(), Error> {
    for (block, rows) in body.call_regions.iter() {
        let block = body.blocks.get(block).ok_or_else(invalid)?;
        let mut seen = false;
        for (index, row) in rows {
            if *index != flux_ty::regions::FUNCTION_LIFETIMES_ARG {
                continue;
            }
            let TerminatorKind::Call { func, .. } = block.terminator.kind else {
                return Err(invalid());
            };
            if seen
                || items.functions.get(func).is_none()
                || sigs.fns.get(func).is_none()
                || flux_ty::regions::named_call_lifetimes(items, sigs, func)
                    != Some(row.len() as u32)
                || row.iter().any(|region| match *region {
                    flux_ty::regions::Region::Param(index) => index as usize >= names,
                    flux_ty::regions::Region::Late(_) => true,
                    flux_ty::regions::Region::Static | flux_ty::regions::Region::Erased => false,
                })
            {
                return Err(Error::new("invalid MIR function lifetime arguments"));
            }
            seen = true;
        }
    }
    Ok(())
}

fn reification_regions(body: &Body, items: &ItemTree, names: usize) -> Result<(), Error> {
    if body.reification_regions.iter().next().is_none() {
        return Ok(());
    }
    let mut definitions = flux_arena::ArenaMap::<LocalDecl, flux_ty::GenericArgs>::new();
    for block in body.blocks.values() {
        for statement in &block.statements {
            if let StatementKind::Assign(dest, value) = &statement.kind
                && body.reification_regions.get(dest.local).is_some()
            {
                let Rvalue::Use(Operand::Const(Constant::Fn { args, .. })) = value else {
                    return Err(Error::new("invalid MIR reification assignment"));
                };
                if !dest.projection.is_empty() || definitions.insert(dest.local, *args).is_some() {
                    return Err(Error::new("duplicate MIR reification assignment"));
                }
            }
        }
        if matches!(&block.terminator.kind,
            TerminatorKind::Call { dest, .. } | TerminatorKind::CallIndirect { dest, .. }
                if body.reification_regions.get(dest.local).is_some())
        {
            return Err(Error::new("invalid MIR reification assignment"));
        }
    }
    for (local, rows) in body.reification_regions.iter() {
        let data = body.locals.get(local).ok_or_else(invalid)?;
        if data.kind != LocalKind::Temp || !matches!(data.ty.kind(), TyKind::FnPtr(_)) {
            return Err(Error::new("invalid MIR reification temporary"));
        }
        let args = definitions.get(local).ok_or_else(invalid)?;
        let mut seen = rustc_hash::FxHashSet::default();
        for (index, row) in rows {
            let &arg = args.get(*index as usize).ok_or_else(invalid)?;
            if !seen.insert(*index) || row.len() != flux_ty::regions::slots(items, arg) {
                return Err(Error::new("invalid MIR reification argument rows"));
            }
            if row.iter().any(|r| match *r {
                flux_ty::regions::Region::Param(i) => i as usize >= names,
                flux_ty::regions::Region::Late(_) => true,
                flux_ty::regions::Region::Static | flux_ty::regions::Region::Erased => false,
            }) {
                return Err(Error::new("invalid MIR reification lifetime"));
            }
        }
    }
    Ok(())
}

pub(super) fn body(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    check_types: bool,
) -> Result<(), Error> {
    let owner = match (body.owner, body.constant_owner) {
        (Some(f), None) => flux_ty::ClosureOwner::Function(f),
        (None, Some(c)) => flux_ty::ClosureOwner::Const(c),
        _ => return Err(invalid()),
    };
    body_for_owner(body, items, sigs, owner, check_types)
}

/// Closure table keys authenticate generated shims whose Body has no owner.
pub(super) fn body_for_owner(
    body: &Body,
    items: &ItemTree,
    sigs: &Signatures,
    owner: flux_ty::ClosureOwner,
    check_types: bool,
) -> Result<(), Error> {
    let loc = match owner {
        flux_ty::ClosureOwner::Function(f) => items.functions.get(f).ok_or_else(invalid)?.loc,
        flux_ty::ClosureOwner::Const(c) => items.consts.get(c).ok_or_else(invalid)?.loc,
    };
    let builtin_owner = match owner {
        flux_ty::ClosureOwner::Function(f) => f.into_raw() < items.core.functions,
        flux_ty::ClosureOwner::Const(c) => c.into_raw() < items.core.consts,
    };
    if body.in_core
        && !(items.has_artifact_builtins() && builtin_owner && items.is_core(loc.module))
    {
        return Err(Error::new(
            "user MIR cannot claim embedded library privileges",
        ));
    }
    let file = loc.file;
    let source_range = |range: TextRange| -> Result<(), Error> {
        let (file, range) = items.source_span(file, range);
        let source = items.files.get(&file).ok_or_else(invalid)?;
        let text = source.text();
        if range.end() as usize > text.len()
            || !text.is_char_boundary(range.start() as usize)
            || !text.is_char_boundary(range.end() as usize)
        {
            return Err(Error::new("invalid MIR source range"));
        }
        Ok(())
    };
    for &offset in body.fn_spans.values() {
        source_range(TextRange::empty(offset))?;
    }
    for block in body.blocks.values() {
        source_range(block.terminator.range)?;
        for statement in &block.statements {
            source_range(statement.range)?;
        }
    }
    for (_, _, range) in &body.region_annotations {
        source_range(*range)?;
    }
    for (_, _, _, range) in &body.region_relations {
        source_range(*range)?;
    }
    for (range, _) in &body.live_drops {
        source_range(*range)?;
    }
    for capture in &body.capture_uses {
        source_range(capture.range)?;
    }
    let mut validated_types = rustc_hash::FxHashSet::default();
    for local in body.locals.values() {
        ty_at(local.ty, items, sigs, 0, &mut validated_types)?;
    }
    for (id, starts) in sigs.variant_starts.iter() {
        let all = sigs.structs.get(id).ok_or_else(invalid)?;
        let count = items
            .structs
            .get(id)
            .and_then(|s| s.variants.as_ref())
            .ok_or_else(invalid)?
            .len();
        if starts.len() != count + 1
            || starts.first() != Some(&0)
            || starts.last().copied() != Some(all.len() as u32)
            || starts.windows(2).any(|w| w[0] > w[1])
        {
            return Err(invalid());
        }
    }
    if let Some(up) = &body.upvars {
        if up.names.len() != up.by_ref.len() || up.names.len() != up.mutable.len() {
            return Err(invalid());
        }
        let capture = body.locals.get(Local::from_raw(1)).ok_or_else(invalid)?.ty;
        let capture = match capture.kind() {
            TyKind::Ref(_, t) => t,
            _ => capture,
        };
        if fields(capture, None, sigs)?.len() != up.names.len() {
            return Err(invalid());
        }
    }
    for inner in &body.inline_constants {
        if inner.owner.is_some()
            || inner.constant_owner.is_some()
            || inner.param_count != 0
            || inner.upvars.is_some()
            || inner.locals.get(Body::RETURN).is_none()
            || !crate::inline_consts::closed(inner)
        {
            return Err(invalid());
        }
        self::body_for_owner(inner, items, sigs, owner, check_types)?;
    }
    for v in &body.promoted {
        value(v, items, sigs, 0)?;
    }
    for (p, q, _, _) in &body.region_relations {
        place(p, body, items, sigs)?;
        place(q, body, items, sigs)?;
    }
    for block in body.blocks.values() {
        for statement in &block.statements {
            match &statement.kind {
                StatementKind::Assign(dest, r) => {
                    let dest_ty = place(dest, body, items, sigs)?;
                    match r {
                        Rvalue::WithAddr(pointer, address) => {
                            operand(pointer, body, items, sigs)?;
                            operand(address, body, items, sigs)?;
                            if body.operand_ty(pointer, sigs) != dest_ty
                                || !matches!(dest_ty.kind(), TyKind::RawPtr(..))
                                || body.operand_ty(address, sigs) != Ty::int(flux_ty::IntTy::Usize)
                            {
                                return Err(invalid());
                            }
                        }
                        Rvalue::OffsetOf(ty, path) => {
                            let mut ty = *ty;
                            if path.is_empty() || dest_ty != Ty::int(flux_ty::IntTy::Usize) {
                                return Err(invalid());
                            }
                            for &index in path {
                                ty = *fields(ty, None, sigs)?
                                    .get(index as usize)
                                    .ok_or_else(invalid)?;
                            }
                        }
                        Rvalue::Aggregate(kind, ops) => {
                            for o in ops {
                                operand(o, body, items, sigs)?;
                            }
                            match kind {
                                AggregateKind::Variant(s, v) => {
                                    variant(items, *s, *v)?;
                                    if fields(dest_ty, Some(*v), sigs)?.len() != ops.len() {
                                        return Err(invalid());
                                    }
                                }
                                AggregateKind::Union(s, f) => {
                                    let def = items.structs.get(*s).ok_or_else(invalid)?;
                                    if !def.is_union
                                        || *f as usize >= def.fields.len()
                                        || ops.len() != 1
                                    {
                                        return Err(invalid());
                                    }
                                }
                                AggregateKind::Struct(s) => {
                                    let def = items.structs.get(*s).ok_or_else(invalid)?;
                                    if def.variants.is_some() || def.fields.len() != ops.len() {
                                        return Err(invalid());
                                    }
                                }
                                AggregateKind::Closure
                                    if fields(dest_ty, None, sigs)?.len() != ops.len() =>
                                {
                                    return Err(invalid());
                                }
                                _ => {}
                            }
                        }
                        Rvalue::Use(o)
                        | Rvalue::UnaryOp(_, o)
                        | Rvalue::Cast(o, _)
                        | Rvalue::Repeat(o, _)
                        | Rvalue::Unsize(o, _) => operand(o, body, items, sigs)?,
                        Rvalue::BinaryOp(_, a, b) => {
                            operand(a, body, items, sigs)?;
                            operand(b, body, items, sigs)?;
                        }
                        Rvalue::InlineAsm { inputs, .. } => {
                            for o in inputs {
                                operand(o, body, items, sigs)?;
                            }
                        }
                        Rvalue::Discriminant(p)
                        | Rvalue::DiscriminantForDrop(p)
                        | Rvalue::Ref { place: p, .. }
                        | Rvalue::Len { place: p, .. } => {
                            place(p, body, items, sigs)?;
                        }
                        Rvalue::Subslice { slice, start, end } => {
                            operand(slice, body, items, sigs)?;
                            operand(start, body, items, sigs)?;
                            operand(end, body, items, sigs)?;
                        }
                    }
                }
                StatementKind::PlaceMention(p) => {
                    place(p, body, items, sigs)?;
                }
                StatementKind::StorageDead(_) => {}
            }
        }
        match &block.terminator.kind {
            TerminatorKind::SwitchInt {
                discr,
                targets,
                variants,
                ..
            } => {
                operand(discr, body, items, sigs)?;
                if let Some(s) = variants {
                    for (v, _) in targets {
                        variant(items, *s, u32::try_from(*v).map_err(|_| invalid())?)?;
                    }
                }
            }
            TerminatorKind::Call {
                func, args, dest, ..
            } => {
                items.functions.get(*func).ok_or_else(invalid)?;
                sigs.fns.get(*func).ok_or_else(invalid)?;
                for o in args {
                    operand(o, body, items, sigs)?;
                }
                place(dest, body, items, sigs)?;
            }
            TerminatorKind::CallIndirect {
                callee, args, dest, ..
            } => {
                operand(callee, body, items, sigs)?;
                for o in args {
                    operand(o, body, items, sigs)?;
                }
                place(dest, body, items, sigs)?;
            }
            TerminatorKind::Drop { place: p, .. } => {
                place(p, body, items, sigs)?;
            }
            TerminatorKind::Assert { cond, msg, .. } => {
                operand(cond, body, items, sigs)?;
                match msg {
                    AssertMessage::DivisionByZero(a) | AssertMessage::RemainderByZero(a) => {
                        operand(a, body, items, sigs)?
                    }
                    AssertMessage::DivisionOverflow(a, b)
                    | AssertMessage::RemainderOverflow(a, b)
                    | AssertMessage::BoundsCheck { len: a, index: b } => {
                        operand(a, body, items, sigs)?;
                        operand(b, body, items, sigs)?;
                    }
                }
            }
            _ => {}
        }
    }
    let names = match owner {
        flux_ty::ClosureOwner::Function(f) => sigs.regions.fns.get(f).map_or(0, |r| r.names.len()),
        flux_ty::ClosureOwner::Const(c) => items.consts[c]
            .owner
            .and_then(flux_hir::Owner::as_impl)
            .and_then(|imp| sigs.regions.impls.get(imp))
            .map_or(0, |r| r.names.len()),
    };
    named_call_regions(body, items, sigs, names)?;
    reification_regions(body, items, names)?;
    if !check_types {
        return Ok(());
    }
    let env = match owner {
        flux_ty::ClosureOwner::Function(f) => flux_solve::traits::projections_of(items, sigs, f),
        flux_ty::ClosureOwner::Const(c) => {
            flux_solve::traits::owner_projections(items, sigs, items.consts[c].owner)
        }
    };
    if !crate::validate::validate(items, sigs, &env, body).is_empty() {
        return Err(Error::new("ill-typed imported MIR"));
    }
    Ok(())
}

// All referenced Values remain borrowed for the complete validation call.
// Addresses are only in-memory DAG keys; they never enter an artifact.
#[derive(Default)]
struct ValueChecks {
    values: rustc_hash::FxHashSet<(usize, u32)>,
    typed: rustc_hash::FxHashSet<(usize, Ty, u32, bool)>,
    types: rustc_hash::FxHashSet<(Ty, u32)>,
}

pub(super) fn value(
    v: &Value,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
) -> Result<(), Error> {
    if matches!(v, Value::Uninit(t) if !matches!(t.kind(), TyKind::Adt(adt, _) if flux_solve::is_lang_struct(items, adt.strukt, "maybe_uninit")))
    {
        return Err(invalid());
    }
    value_at(v, items, sigs, depth, &mut ValueChecks::default())
}

fn value_at(
    v: &Value,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
    memo: &mut ValueChecks,
) -> Result<(), Error> {
    if depth > 128 {
        return Err(invalid());
    }
    let key = (v as *const Value as usize, depth);
    if memo.values.contains(&key) {
        return Ok(());
    }
    match v {
        Value::Uninit(stored) => {
            ty(*stored, items, sigs, depth + 1)?;
        }
        Value::Union { value, .. } => value_at(value, items, sigs, depth + 1, memo)?,
        Value::Variant {
            strukt,
            index,
            fields,
        } => {
            variant(items, *strukt, *index)?;
            let def = &items.structs[*strukt];
            if def
                .variants
                .as_ref()
                .and_then(|v| v.get(*index as usize))
                .is_none_or(|v| v.fields.len() != fields.len())
            {
                return Err(invalid());
            }
            for field in fields.iter() {
                value_at(field, items, sigs, depth + 1, memo)?;
            }
        }
        Value::Aggregate(fields) => {
            for field in fields.iter() {
                value_at(field, items, sigs, depth + 1, memo)?;
            }
        }
        Value::Function(f) => {
            items.functions.get(f.function).ok_or_else(invalid)?;
            sigs.fns.get(f.function).ok_or_else(invalid)?;
        }
        Value::Static(id) => {
            items.consts.get(*id).ok_or_else(invalid)?;
            sigs.consts.get(*id).ok_or_else(invalid)?;
        }
        Value::StaticPart { id, path } => {
            items.consts.get(*id).ok_or_else(invalid)?;
            let mut ty = *sigs.consts.get(*id).ok_or_else(invalid)?;
            let mut selected = None;
            for p in path.iter() {
                match *p {
                    StaticProjection::Downcast(v) => {
                        let TyKind::Adt(adt, _) = ty.kind() else {
                            return Err(invalid());
                        };
                        variant(items, adt.strukt, v)?;
                        selected = Some(v);
                        continue;
                    }
                    StaticProjection::Field(i) => {
                        ty = *fields(ty, selected, sigs)?
                            .get(i as usize)
                            .ok_or_else(invalid)?
                    }
                    StaticProjection::Index(i) => {
                        if ty.array_len().is_some_and(|n| i > n) {
                            return Err(invalid());
                        }
                        ty = ty.sequence_elem().ok_or_else(invalid)?;
                    }
                    StaticProjection::Subslice { from, len } => {
                        if from.checked_add(len).is_none()
                            || ty.array_len().is_some_and(|n| from + len > n)
                        {
                            return Err(invalid());
                        }
                        ty = Ty::array(ty.sequence_elem().ok_or_else(invalid)?, len);
                    }
                }
                selected = None;
            }
        }
        Value::Ref(p) | Value::InteriorRef(p) => value_at(p, items, sigs, depth + 1, memo)?,
        Value::DynPointer {
            pointer,
            concrete,
            object,
            ..
        } => {
            ty_at(*concrete, items, sigs, 0, &mut memo.types)?;
            ty_at(*object, items, sigs, 0, &mut memo.types)?;
            if !matches!(object.kind(), TyKind::Dynamic(_)) {
                return Err(invalid());
            }
            value_at(pointer, items, sigs, depth + 1, memo)?;
        }
        Value::PointerView {
            pointer,
            source,
            target,
        }
        | Value::PointerOffset {
            pointer,
            source,
            target,
            ..
        } => {
            ty_at(*source, items, sigs, 0, &mut memo.types)?;
            ty_at(*target, items, sigs, 0, &mut memo.types)?;
            value_at(pointer, items, sigs, depth + 1, memo)?;
        }
        Value::FramePointer { .. } | Value::LocalRef { .. } => return Err(invalid()),
        _ => {}
    }
    memo.values.insert(key);
    Ok(())
}

pub(super) fn value_for(
    v: &Value,
    expected: Ty,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
) -> Result<(), Error> {
    value_for_at(v, expected, items, sigs, depth, &mut ValueChecks::default())
}

fn value_for_at(
    v: &Value,
    expected: Ty,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
    memo: &mut ValueChecks,
) -> Result<(), Error> {
    value_for_storage(v, expected, items, sigs, depth, memo, false)
}

fn value_for_storage(
    v: &Value,
    expected: Ty,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
    memo: &mut ValueChecks,
    storage: bool,
) -> Result<(), Error> {
    if depth > 128 {
        return Err(invalid());
    }
    let key = (v as *const Value as usize, expected, depth, storage);
    let storage = storage
        || matches!(expected.kind(), TyKind::Adt(adt, _) if flux_solve::is_lang_struct(items, adt.strukt, "maybe_uninit"));
    if memo.typed.contains(&key) {
        return Ok(());
    }
    value_at(v, items, sigs, depth, memo)?;
    match (v, expected.kind()) {
        (Value::Uninit(ty), _) if *ty == expected && storage => {}
        (Value::Int { bits, ty }, TyKind::Int(int))
            if *ty == int && (int.bits() == 128 || *bits >> int.bits() == 0) => {}
        (Value::Int { bits, .. }, TyKind::Char)
            if u32::try_from(*bits).ok().and_then(char::from_u32).is_some() => {}
        (Value::Bool(_), TyKind::Bool) => {}
        (Value::Float { ty, .. }, TyKind::Float(float)) if *ty == float => {}
        (Value::Union { field, value }, TyKind::Adt(adt, _))
            if sigs.unions.contains(&adt.strukt) =>
        {
            let types = fields(expected, None, sigs)?;
            let ty = types.get(*field as usize).copied().ok_or_else(invalid)?;
            value_for_storage(value, ty, items, sigs, depth + 1, memo, storage)?;
        }
        (Value::Aggregate(values), TyKind::Array(element, len)) => {
            if len.as_usize().is_some_and(|n| n != values.len() as u64) {
                return Err(invalid());
            }
            for v in values.iter() {
                value_for_storage(v, element, items, sigs, depth + 1, memo, storage)?;
            }
        }
        (Value::Aggregate(values), TyKind::Slice(element)) => {
            for v in values.iter() {
                value_for_storage(v, element, items, sigs, depth + 1, memo, storage)?;
            }
        }
        (Value::Aggregate(values), TyKind::Str | TyKind::CStr) => {
            for v in values.iter() {
                value_for_storage(
                    v,
                    Ty::int(flux_ty::IntTy::U8),
                    items,
                    sigs,
                    depth + 1,
                    memo,
                    storage,
                )?;
            }
        }
        (Value::Aggregate(values), TyKind::Tuple(_) | TyKind::Adt(..) | TyKind::Closure(..)) => {
            let fields = fields(expected, None, sigs)?;
            if fields.len() != values.len() {
                return Err(invalid());
            }
            for (v, t) in values.iter().zip(fields) {
                value_for_storage(v, t, items, sigs, depth + 1, memo, storage)?;
            }
        }
        (
            Value::Variant {
                strukt,
                index,
                fields: values,
            },
            TyKind::Adt(adt, _),
        ) if *strukt == adt.strukt => {
            let fields = fields(expected, Some(*index), sigs)?;
            if fields.len() != values.len() {
                return Err(invalid());
            }
            for (v, t) in values.iter().zip(fields) {
                value_for_storage(v, t, items, sigs, depth + 1, memo, storage)?;
            }
        }
        (Value::Ref(p) | Value::InteriorRef(p), TyKind::Ref(_, t) | TyKind::RawPtr(_, t)) => {
            value_for_at(p, t, items, sigs, depth + 1, memo)?
        }
        (Value::Function(_), TyKind::FnPtr(_) | TyKind::FnDef(_)) => {}
        (
            Value::Static(_)
            | Value::StaticPart { .. }
            | Value::RawAddr(_)
            | Value::PointerView { .. }
            | Value::PointerOffset { .. }
            | Value::DynPointer { .. },
            TyKind::Ref(..) | TyKind::RawPtr(..),
        ) => {}
        (_, TyKind::Param(_) | TyKind::Alias(_) | TyKind::Opaque(_)) => {}
        _ => return Err(Error::new("constant value does not match its type")),
    }
    memo.typed.insert(key);
    Ok(())
}

#[cfg(test)]
mod dag_tests {
    use super::*;
    #[test]
    fn type_validation_visits_distinct_dag_nodes() {
        let mut ty = Ty::BOOL;
        for _ in 0..40 {
            ty = Ty::tuple(&[ty, ty]);
        }
        let mut validated = rustc_hash::FxHashSet::default();
        ty_at(
            ty,
            &ItemTree::default(),
            &Signatures::default(),
            0,
            &mut validated,
        )
        .unwrap();
        assert_eq!(validated.len(), 41);
    }
    #[test]
    fn shallow_validation_does_not_bypass_deeper_depth_or_item_checks() {
        let shared = Ty::raw_ptr(flux_ty::Mutability::Not, Ty::BOOL);
        let mut deep = shared;
        for _ in 0..127 {
            deep = Ty::raw_ptr(flux_ty::Mutability::Not, deep);
        }
        let root = Ty::tuple(&[shared, deep]);
        let mut validated = rustc_hash::FxHashSet::default();
        assert!(
            ty_at(
                root,
                &ItemTree::default(),
                &Signatures::default(),
                0,
                &mut validated
            )
            .is_err()
        );
        assert!(validated.contains(&(shared, 1)));
        let invalid = Ty::adt(
            flux_hir::StructId::from_raw(u32::MAX),
            flux_intern::Symbol::intern("missing"),
            &[],
        );
        assert!(
            ty_at(
                invalid,
                &ItemTree::default(),
                &Signatures::default(),
                0,
                &mut validated
            )
            .is_err()
        );
        assert!(!validated.contains(&(invalid, 0)));
    }
}

#[cfg(test)]
mod value_dag_tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn uninitialized_metadata_requires_the_matching_wrapper() {
        let (_, _, hir) = flux_hir::lower_text(
            "uninit.rs",
            "const X:core::mem::MaybeUninit<u32>=core::mem::MaybeUninit::uninit();",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let (id, _) = hir
            .items
            .consts
            .iter()
            .find(|(_, c)| c.name == Some(flux_intern::Symbol::intern("X")))
            .unwrap();
        let expected = typed.sigs.consts[id];
        let value = Value::Uninit(expected);
        assert!(value_for(&value, expected, &hir.items, &typed.sigs, 0).is_ok());
        assert!(value_for(&value, Ty::BOOL, &hir.items, &typed.sigs, 0).is_err());
        assert!(super::value(&Value::Uninit(Ty::BOOL), &hir.items, &typed.sigs, 0).is_err());
    }

    #[test]
    fn forty_level_shared_reference_dag_validates_each_stored_node_once() {
        let mut v = Value::Bool(false);
        let mut ty = Ty::BOOL;
        for _ in 0..40 {
            let shared = Arc::new(v);
            v = Value::Aggregate(vec![Value::Ref(shared.clone()), Value::Ref(shared)].into());
            let reference = Ty::new(TyKind::Ref(flux_ty::Mutability::Not, ty));
            ty = Ty::tuple(&[reference, reference]);
        }
        let mut memo = ValueChecks::default();
        value_for_at(
            &v,
            ty,
            &ItemTree::default(),
            &Signatures::default(),
            0,
            &mut memo,
        )
        .unwrap();
        assert_eq!(
            memo.values.len(),
            121,
            "40 aggregates, 80 references and one leaf"
        );
        assert_eq!(memo.typed.len(), 121);
    }

    #[test]
    fn value_memo_keeps_type_and_depth_checks() {
        let items = ItemTree::default();
        let sigs = Signatures::default();
        let v = Value::Bool(false);
        let mut memo = ValueChecks::default();
        value_for_at(&v, Ty::BOOL, &items, &sigs, 0, &mut memo).unwrap();
        assert!(
            value_for_at(&v, Ty::int(flux_ty::IntTy::U8), &items, &sigs, 0, &mut memo).is_err()
        );
        let shared = Arc::new(Value::Ref(Arc::new(Value::Bool(false))));
        let shared_key = shared.as_ref() as *const Value as usize;
        let mut deep = Value::Ref(shared.clone());
        for _ in 0..126 {
            deep = Value::Ref(Arc::new(deep));
        }
        let root = Value::Aggregate(vec![Value::Ref(shared), deep].into());
        assert!(value_at(&root, &items, &sigs, 0, &mut memo).is_err());
        assert!(
            memo.values.contains(&(shared_key, 2)),
            "shallow reference succeeded first"
        );
        assert!(
            !memo.values.contains(&(shared_key, 128)),
            "deep reference must not skip its overflowing child"
        );
    }
}

#[cfg(test)]
mod inline_constant_tests {
    use super::*;

    #[test]
    fn imported_inline_frames_reject_corrupt_structure() {
        let (_, _, hir) = flux_hir::lower_text(
            "inline.rs",
            "const fn twice(x:u32)->u32{x*2}pub fn value<T>()->&'static[u32]{const{&[twice(7)]}}",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = crate::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let (id, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| {
                f.name.as_str() == "value"
                    && f.loc.module == hir.items.module_of(hir.items.main_crate())
            })
            .unwrap();
        let original = &mir.bodies[id];
        assert_eq!(original.inline_constants.len(), 1);
        assert!(body(original, &hir.items, &typed.sigs, true).is_ok());
        for corruption in ["index", "type", "owner", "parameters", "generic"] {
            let mut changed = original.clone();
            match corruption {
                "index" | "type" => {
                    let mut found = false;
                    for block in changed.blocks.values_mut() {
                        for statement in &mut block.statements {
                            if let StatementKind::Assign(
                                _,
                                Rvalue::Use(Operand::Const(Constant::Inline { index, ty })),
                            ) = &mut statement.kind
                            {
                                if corruption == "index" {
                                    *index = u32::MAX;
                                } else {
                                    *ty = Ty::BOOL;
                                }
                                found = true;
                            }
                        }
                    }
                    assert!(found);
                }
                "owner" => changed.inline_constants[0].owner = Some(id),
                "parameters" => changed.inline_constants[0].param_count = 1,
                "generic" => {
                    changed.inline_constants[0].locals[Body::RETURN].ty =
                        Ty::param(0, flux_intern::Symbol::intern("T"))
                }
                _ => unreachable!(),
            }
            assert!(
                body(&changed, &hir.items, &typed.sigs, true).is_err(),
                "{corruption}"
            );
        }
    }
}

#[cfg(test)]
mod named_call_tests {
    use super::*;
    #[test]
    fn imported_early_lifetimes_reject_invalid_binders() {
        let (_, _, hir) = flux_hir::lower_text(
            "early.rs",
            "fn first<'a:'a>(x:&'a u8)->&'a u8{x}fn forward<'s>(x:&'s u8)->&'s u8{first::<'s>(x)}",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let mir = crate::build_file(&hir, &typed);
        assert!(mir.diagnostics.is_empty(), "{:?}", mir.diagnostics);
        let (id, _) = hir
            .items
            .functions
            .iter()
            .find(|(_, f)| f.name.as_str() == "forward")
            .unwrap();
        let original = &mir.bodies[id];
        let (block, _) = original
            .call_regions
            .iter()
            .find(|(_, rows)| {
                rows.iter()
                    .any(|(index, _)| *index == flux_ty::regions::FUNCTION_LIFETIMES_ARG)
            })
            .unwrap();
        assert!(body(original, &hir.items, &typed.sigs, true).is_ok());
        for corruption in ["count", "param", "late", "duplicate", "terminator"] {
            let mut changed = original.clone();
            let rows = changed.call_regions.get_mut(block).unwrap();
            let (_, row) = rows
                .iter_mut()
                .find(|(index, _)| *index == flux_ty::regions::FUNCTION_LIFETIMES_ARG)
                .unwrap();
            match corruption {
                "count" => *row = Box::default(),
                "param" => row[0] = flux_ty::regions::Region::Param(u32::MAX),
                "late" => row[0] = flux_ty::regions::Region::Late(0),
                "duplicate" => {
                    let duplicate = (flux_ty::regions::FUNCTION_LIFETIMES_ARG, row.clone());
                    rows.push(duplicate);
                }
                "terminator" => changed.blocks[block].terminator.kind = TerminatorKind::Return,
                _ => unreachable!(),
            }
            assert!(
                body(&changed, &hir.items, &typed.sigs, true).is_err(),
                "{corruption}"
            );
        }
    }
}

#[cfg(test)]
mod union_storage_tests {
    use super::*;
    #[test]
    fn stored_union_field_is_validated() {
        let (_, _, hir) = flux_hir::lower_text(
            "union.rs",
            "#[repr(C)]union Bits{bytes:[u8;1],flag:bool}fn main(){}",
        );
        let typed = crate::check_file(&hir);
        assert!(typed.diagnostics.is_empty(), "{:?}", typed.diagnostics);
        let (id, data) = hir
            .items
            .structs
            .iter()
            .find(|(_, data)| data.name.as_str() == "Bits")
            .unwrap();
        let ty = Ty::adt(id, data.name, &[]);
        let stored = Value::Aggregate(
            vec![Value::Int {
                bits: 2,
                ty: flux_ty::IntTy::U8,
            }]
            .into(),
        );
        let valid = Value::Union {
            field: 0,
            value: std::sync::Arc::new(stored),
        };
        assert!(
            value_for_at(
                &valid,
                ty,
                &hir.items,
                &typed.sigs,
                0,
                &mut ValueChecks::default()
            )
            .is_ok()
        );
        for field in [1, u32::MAX] {
            let changed = Value::Union {
                field,
                value: std::sync::Arc::new(Value::Bool(false)),
            };
            assert_eq!(
                value_for_at(
                    &changed,
                    ty,
                    &hir.items,
                    &typed.sigs,
                    0,
                    &mut ValueChecks::default()
                )
                .is_ok(),
                field == 1
            );
        }
        let wrong = Value::Union {
            field: 1,
            value: std::sync::Arc::new(Value::Int {
                bits: 2,
                ty: flux_ty::IntTy::U8,
            }),
        };
        assert!(
            value_for_at(
                &wrong,
                ty,
                &hir.items,
                &typed.sigs,
                0,
                &mut ValueChecks::default()
            )
            .is_err()
        );
        assert!(
            value_for_at(
                &valid,
                Ty::BOOL,
                &hir.items,
                &typed.sigs,
                0,
                &mut ValueChecks::default()
            )
            .is_err()
        );
    }
}
