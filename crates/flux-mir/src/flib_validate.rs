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
            items.traits.get(d.trait_).ok_or_else(invalid)?;
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
    typed: rustc_hash::FxHashSet<(usize, Ty, u32)>,
    types: rustc_hash::FxHashSet<(Ty, u32)>,
}

pub(super) fn value(
    v: &Value,
    items: &ItemTree,
    sigs: &Signatures,
    depth: u32,
) -> Result<(), Error> {
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
        Value::LocalRef { .. } => return Err(invalid()),
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
    if depth > 128 {
        return Err(invalid());
    }
    let key = (v as *const Value as usize, expected, depth);
    if memo.typed.contains(&key) {
        return Ok(());
    }
    value_at(v, items, sigs, depth, memo)?;
    match (v, expected.kind()) {
        (Value::Int { bits, ty }, TyKind::Int(int))
            if *ty == int && (int.bits() == 128 || *bits >> int.bits() == 0) => {}
        (Value::Int { bits, .. }, TyKind::Char)
            if u32::try_from(*bits).ok().and_then(char::from_u32).is_some() => {}
        (Value::Bool(_), TyKind::Bool) => {}
        (Value::Float { ty, .. }, TyKind::Float(float)) if *ty == float => {}
        (Value::Aggregate(values), TyKind::Array(element, len)) => {
            if len.as_usize().is_some_and(|n| n != values.len() as u64) {
                return Err(invalid());
            }
            for v in values.iter() {
                value_for_at(v, element, items, sigs, depth + 1, memo)?;
            }
        }
        (Value::Aggregate(values), TyKind::Slice(element)) => {
            for v in values.iter() {
                value_for_at(v, element, items, sigs, depth + 1, memo)?;
            }
        }
        (Value::Aggregate(values), TyKind::Str | TyKind::CStr) => {
            for v in values.iter() {
                value_for_at(v, Ty::int(flux_ty::IntTy::U8), items, sigs, depth + 1, memo)?;
            }
        }
        (Value::Aggregate(values), TyKind::Tuple(_) | TyKind::Adt(..) | TyKind::Closure(..)) => {
            let fields = fields(expected, None, sigs)?;
            if fields.len() != values.len() {
                return Err(invalid());
            }
            for (v, t) in values.iter().zip(fields) {
                value_for_at(v, t, items, sigs, depth + 1, memo)?;
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
                value_for_at(v, t, items, sigs, depth + 1, memo)?;
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
