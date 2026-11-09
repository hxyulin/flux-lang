//! Live frame allocations and typed, bounded writes through constant pointers.
use super::*;

pub(super) struct FrameAllocation {
    pub ty: Ty,
    pub value: Value,
    pub frozen: bool,
}

// Retain the original allocation so pointer identity cannot be reused while
// the materialization cache still contains its key.
struct MaterializedFields {
    _original: Arc<[Value]>,
    result: Arc<[Value]>,
    changed: bool,
}
type MaterializationCache = FxHashMap<(usize, Ty), MaterializedFields>;

impl Evaluator<'_> {
    pub(super) fn frame_allocation(
        &self,
        origin: LocalOrigin,
        range: TextRange,
    ) -> Result<&FrameAllocation, EvalError> {
        self.frame_memory
            .get(&origin.token)
            .filter(|_| self.live_borrows.contains(&origin.token))
            .ok_or_else(|| error(range, "dangling reference to local constant storage".into()))
    }
    pub(super) fn sync_frame(&self, locals: &mut [Option<Value>]) {
        if let Some(frame) = self.frame_slots.last() {
            for &(index, token) in frame {
                if let Some(allocation) = self.frame_memory.get(&token) {
                    locals[index] = Some(allocation.value.clone());
                }
            }
        }
    }
    /// Drop temporary snapshots before a callee mutates the canonical storage.
    pub(super) fn release_frame(&self, locals: &mut [Option<Value>]) {
        if let Some(frame) = self.frame_slots.last() {
            for &(index, _) in frame {
                locals[index] = None;
            }
        }
    }
    pub(super) fn frame_assign(
        &mut self,
        body: &Body,
        locals: &mut [Option<Value>],
        generics: GenericArgs,
        dest: &Place,
        value: Value,
        range: TextRange,
    ) -> Result<(), EvalError> {
        self.sync_frame(locals);
        if dest.projection.first() == Some(&ProjectionElem::Deref) {
            let pointer = locals[dest.local.index()]
                .as_ref()
                .ok_or_else(|| unsupported_view(range))?
                .clone();
            let source = body.locals[dest.local].ty.subst(generics);
            let ty = body.place_ty(dest, self.sigs).subst(generics);
            let pointer = self.projected_address(
                pointer,
                source,
                &dest.projection[1..],
                locals,
                Ty::raw_ptr(flux_ty::Mutability::Mut, ty),
                range,
            )?;
            self.release_frame(locals);
            return self.frame_store(
                &pointer,
                Ty::raw_ptr(flux_ty::Mutability::Mut, ty),
                value,
                range,
            );
        }
        let token = self
            .local_borrows
            .last()
            .and_then(|f| f.get(dest.local.index()))
            .copied()
            .flatten()
            .map(|o| o.token)
            .filter(|t| self.frame_memory.contains_key(t));
        if let Some(token) = token {
            if self.frame_memory[&token].frozen {
                return Err(error(
                    range,
                    "constant storage has a shared reference".into(),
                ));
            }
            locals[dest.local.index()] = None;
            locals[dest.local.index()] = Some(std::mem::replace(
                &mut self.frame_memory.get_mut(&token).unwrap().value,
                Value::unit(),
            ));
            let result = write(locals, dest, value, range);
            self.frame_memory.get_mut(&token).unwrap().value =
                locals[dest.local.index()].take().unwrap();
            result
        } else {
            write(locals, dest, value, range)
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn frame_address(
        &mut self,
        body: &Body,
        locals: &[Option<Value>],
        generics: GenericArgs,
        place: &Place,
        raw: bool,
        mutable: flux_ty::Mutability,
        range: TextRange,
    ) -> Result<Value, EvalError> {
        let target_ty = body.place_ty(place, self.sigs).subst(generics);
        if mutable == flux_ty::Mutability::Mut {
            let empty = match target_ty.kind() {
                TyKind::Array(_, len) => len.as_usize() == Some(0),
                TyKind::Slice(_) => true,
                _ => false,
            };
            if empty
                && place.projection.as_ref() == [ProjectionElem::Deref]
                && locals
                    .get(place.local.index())
                    .and_then(Option::as_ref)
                    .is_some_and(|v| !Self::is_frame_pointer(v))
            {
                let value = self.operand(locals, generics, &Operand::Copy(place.clone()), range)?;
                if matches!(&value,Value::Aggregate(fields) if fields.is_empty()) {
                    return Ok(Value::Ref(Arc::new(value)));
                }
            }
            if empty
                && place.projection.is_empty()
                && body.locals[place.local].kind.is_temp()
                && let Some(Some(value @ Value::Aggregate(fields))) =
                    locals.get(place.local.index())
                && fields.is_empty()
            {
                return Ok(Value::Ref(Arc::new(value.clone())));
            }
        }
        let target = if raw {
            Ty::raw_ptr(mutable, target_ty)
        } else {
            Ty::new(TyKind::Ref(mutable, target_ty))
        };
        let (pointer, source, projection) =
            if place.projection.first() == Some(&ProjectionElem::Deref) {
                (
                    locals[place.local.index()]
                        .as_ref()
                        .ok_or_else(|| unsupported_view(range))?
                        .clone(),
                    body.locals[place.local].ty.subst(generics),
                    &place.projection[1..],
                )
            } else {
                let value = locals[place.local.index()]
                    .as_ref()
                    .ok_or_else(|| error(range, "use of an uninitialized value".into()))?
                    .clone();
                let ty = body.locals[place.local].ty.subst(generics);
                if self.inline_interior(&value, ty) {
                    return Err(error(
                        range,
                        "borrowing UnsafeCell in constants is not supported yet".into(),
                    ));
                }
                let writable = matches!(
                    body.locals[place.local].kind,
                    crate::LocalKind::Param | crate::LocalKind::User { mutable: true, .. }
                ) || mutable == flux_ty::Mutability::Mut;
                let origin = self.local_origin(place.local, writable);
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    self.frame_memory.entry(origin.token)
                {
                    entry.insert(FrameAllocation {
                        ty,
                        value,
                        frozen: false,
                    });
                    self.frame_slots
                        .last_mut()
                        .unwrap()
                        .push((place.local.index(), origin.token));
                }
                (
                    Value::FramePointer {
                        origin,
                        ty,
                        writable: mutable == flux_ty::Mutability::Mut,
                    },
                    Ty::raw_ptr(mutable, ty),
                    &place.projection[..],
                )
            };
        let mut pointer =
            self.projected_address(pointer, source, projection, locals, target, range)?;
        if mutable == flux_ty::Mutability::Not {
            self.readonly_frame_pointer(&mut pointer, !raw);
        }
        if !raw {
            let value = self.pointer_value(&pointer, target, target, range)?;
            self.initialized(&value, target_ty, range)?;
        }
        Ok(pointer)
    }
    pub(super) fn frame_slice_view(
        &mut self,
        value: &Value,
        from: Ty,
        to: Ty,
        range: TextRange,
    ) -> Result<Option<Value>, EvalError> {
        if Self::is_frame_pointer(value)
            && matches!(pointer_pointee(to).map(Ty::kind), Some(TyKind::Slice(_)))
            && let Some(len) = pointer_pointee(from).and_then(Ty::array_len)
        {
            let (pointer, source, offset) = self.pointer_origin(value.clone(), from, range)?;
            return Ok(Some(Value::PointerOffset {
                pointer: Arc::new(pointer),
                source,
                offset,
                target: to,
                len: Some(len),
            }));
        }
        Ok(None)
    }
    pub(super) fn is_frame_pointer(value: &Value) -> bool {
        match value {
            Value::FramePointer { .. } => true,
            Value::PointerView { pointer, .. } | Value::PointerOffset { pointer, .. } => {
                Self::is_frame_pointer(pointer)
            }
            _ => false,
        }
    }
    fn readonly_frame_pointer(&mut self, value: &mut Value, freeze: bool) {
        match value {
            Value::FramePointer {
                origin, writable, ..
            } => {
                *writable = false;
                if freeze && let Some(allocation) = self.frame_memory.get_mut(&origin.token) {
                    allocation.frozen = true;
                }
            }
            Value::PointerView { pointer, .. } | Value::PointerOffset { pointer, .. } => {
                self.readonly_frame_pointer(Arc::make_mut(pointer), freeze)
            }
            _ => {}
        }
    }
    pub(super) fn frame_store(
        &mut self,
        pointer: &Value,
        pointer_ty: Ty,
        value: Value,
        range: TextRange,
    ) -> Result<(), EvalError> {
        let target = pointer_pointee(pointer_ty).ok_or_else(|| unsupported_view(range))?;
        self.initialized(&value, target, range)?;
        let (base, _, offset) = self.pointer_origin(pointer.clone(), pointer_ty, range)?;
        let Value::FramePointer {
            origin,
            writable: true,
            ..
        } = base
        else {
            return Err(error(
                range,
                "constant writes require mutable local storage".into(),
            ));
        };
        let allocation = self.frame_allocation(origin, range)?;
        if allocation.frozen || !origin.mutable {
            return Err(error(
                range,
                "constant writes require mutable local storage".into(),
            ));
        }
        let ty = allocation.ty;
        let layout = self.view_layout(ty, range)?;
        let target_layout = self.view_layout(target, range)?;
        if offset
            .checked_add(target_layout.size)
            .is_none_or(|end| end > layout.size)
        {
            return Err(error(
                range,
                "constant pointer write is out of bounds".into(),
            ));
        }
        if layout.align < target_layout.align || offset % target_layout.align != 0 {
            return Err(error(range, "unaligned constant pointer write".into()));
        }
        let mut allocation = self.frame_memory.remove(&origin.token).unwrap();
        let result = self.write_memory(&mut allocation.value, ty, offset, target, value, range, 0);
        self.frame_memory.insert(origin.token, allocation);
        result
    }
    pub(super) fn read_marker(
        &mut self,
        value: Value,
        range: TextRange,
    ) -> Result<Value, EvalError> {
        if let Value::Uninit(ty) = value {
            if self.maybe_uninit(ty) {
                return Ok(value);
            }
            if self.view_layout(ty, range)?.size == 0 {
                return self.zero_sized(ty, range);
            }
            return Err(error(range, "use of an uninitialized value".into()));
        }
        Ok(value)
    }
    fn maybe_uninit(&self, ty: Ty) -> bool {
        matches!(ty.kind(), TyKind::Adt(adt, _) if flux_solve::is_lang_struct(self.items, adt.strukt, "maybe_uninit"))
    }
    pub(super) fn materialize_zero_sized(
        &mut self,
        value: &mut Value,
        ty: Ty,
        range: TextRange,
    ) -> Result<(), EvalError> {
        self.materialize_at(value, ty, range, &mut FxHashMap::default(), 0)
            .map(|_| ())
    }
    fn materialize_at(
        &mut self,
        value: &mut Value,
        ty: Ty,
        range: TextRange,
        memo: &mut MaterializationCache,
        depth: u32,
    ) -> Result<bool, EvalError> {
        if depth > DEPTH_LIMIT {
            return Err(unsupported_view(range));
        }
        if self.maybe_uninit(ty) {
            return Ok(false);
        }
        if matches!(value, Value::Uninit(_)) {
            let layout = self.view_layout(ty, range)?;
            if layout.size == 0 && !layout.uninhabited {
                *value = self.zero_sized(ty, range)?;
                return Ok(true);
            }
            return Ok(false);
        }
        let Value::Aggregate(fields) = value else {
            return Ok(false);
        };
        let key = (Arc::as_ptr(fields) as *const Value as usize, ty);
        if let Some(cached) = memo.get(&key) {
            *fields = cached.result.clone();
            return Ok(cached.changed);
        }
        let original = fields.clone();
        let types = if let TyKind::Array(element, _) = ty.kind() {
            vec![element; fields.len()]
        } else {
            flux_solve::fields(self.sigs, ty)
                .unwrap_or_default()
                .to_vec()
        };
        let mut result = None;
        for (index, field_ty) in types.into_iter().enumerate() {
            let mut field = fields[index].clone();
            if self.materialize_at(&mut field, field_ty, range, memo, depth + 1)? {
                result.get_or_insert_with(|| fields.to_vec())[index] = field;
            }
        }
        let changed = result.is_some();
        if let Some(result) = result {
            *fields = result.into();
        }
        memo.insert(
            key,
            MaterializedFields {
                _original: original,
                result: fields.clone(),
                changed,
            },
        );
        Ok(changed)
    }
    pub(super) fn initialized(
        &mut self,
        value: &Value,
        ty: Ty,
        range: TextRange,
    ) -> Result<(), EvalError> {
        self.initialized_at(value, ty, range, &mut FxHashSet::default(), 0)
    }
    fn initialized_at(
        &mut self,
        value: &Value,
        ty: Ty,
        range: TextRange,
        memo: &mut FxHashSet<(usize, Ty, u32)>,
        depth: u32,
    ) -> Result<(), EvalError> {
        if depth > DEPTH_LIMIT {
            return Err(unsupported_view(range));
        }
        if self.maybe_uninit(ty) {
            return Ok(());
        }
        let (fields, types, variant) = match value {
            Value::Uninit(_) => return Err(error(range, "use of an uninitialized value".into())),
            Value::Aggregate(fields) => (
                fields,
                if let TyKind::Array(element, _) = ty.kind() {
                    vec![element; fields.len()]
                } else {
                    flux_solve::fields(self.sigs, ty)
                        .unwrap_or_default()
                        .to_vec()
                },
                u32::MAX,
            ),
            Value::Variant { fields, index, .. } => (
                fields,
                flux_solve::variant_fields(self.sigs, ty, *index)
                    .unwrap_or_default()
                    .to_vec(),
                *index,
            ),
            _ => return Ok(()),
        };
        let key = (Arc::as_ptr(fields) as *const Value as usize, ty, variant);
        if memo.contains(&key) {
            return Ok(());
        }
        for (value, ty) in fields.iter().zip(types) {
            self.initialized_at(value, ty, range, memo, depth + 1)?;
        }
        memo.insert(key);
        Ok(())
    }
    pub(super) fn expand_storage(&mut self, ty: Ty, range: TextRange) -> Result<Value, EvalError> {
        if self.view_layout(ty, range)?.uninhabited {
            return Err(error(range, "invalid uninhabited constant value".into()));
        }
        let types = if let TyKind::Array(element, length) = ty.kind() {
            let length = length.as_usize().ok_or_else(|| unsupported_view(range))?;
            self.charge_view(length, range)?;
            vec![element; length as usize]
        } else {
            flux_solve::fields(self.sigs, ty)
                .ok_or_else(|| error(range, "use of an uninitialized value".into()))?
                .to_vec()
        };
        self.charge_view(types.len() as u64, range)?;
        Ok(Value::Aggregate(
            types.into_iter().map(Value::Uninit).collect(),
        ))
    }
    fn zero_sized(&mut self, ty: Ty, range: TextRange) -> Result<Value, EvalError> {
        let layout = self.view_layout(ty, range)?;
        if layout.uninhabited || layout.size != 0 {
            return Err(error(range, "use of an uninitialized value".into()));
        }
        if let TyKind::Adt(adt, _) = ty.kind()
            && let Some(variants) = flux_solve::variants(self.sigs, ty)
        {
            for (index, types) in variants.iter().enumerate() {
                let mut values = Vec::new();
                let mut inhabited = true;
                for &field in *types {
                    if self.view_layout(field, range)?.uninhabited {
                        inhabited = false;
                        break;
                    }
                    values.push(self.zero_sized(field, range)?);
                }
                if inhabited {
                    return Ok(Value::Variant {
                        strukt: adt.strukt,
                        index: index as u32,
                        fields: values.into(),
                    });
                }
            }
            return Err(error(range, "invalid uninhabited constant value".into()));
        }
        let value = self.expand_storage(ty, range)?;
        let Value::Aggregate(fields) = value else {
            unreachable!()
        };
        let fields = fields
            .iter()
            .map(|v| {
                let Value::Uninit(ty) = v else { unreachable!() };
                self.zero_sized(*ty, range)
            })
            .collect::<Result<Arc<[_]>, _>>()?;
        Ok(Value::Aggregate(fields))
    }
    #[allow(clippy::too_many_arguments)]
    fn write_memory(
        &mut self,
        stored: &mut Value,
        ty: Ty,
        offset: u64,
        target: Ty,
        value: Value,
        range: TextRange,
        depth: u32,
    ) -> Result<(), EvalError> {
        if depth > DEPTH_LIMIT {
            return Err(unsupported_view(range));
        }
        if offset == 0 && ty == target {
            *stored = value;
            return Ok(());
        }
        if matches!(stored, Value::Uninit(_)) {
            *stored = self.expand_storage(ty, range)?;
        }
        let Value::Aggregate(fields) = stored else {
            return Err(error(
                range,
                "partial scalar writes in constants are not supported yet".into(),
            ));
        };
        let target_size = self.view_layout(target, range)?.size;
        if let TyKind::Array(element, _) = ty.kind() {
            let stride = self.view_layout(element, range)?.size;
            let index = offset.checked_div(stride).unwrap_or(0);
            let within = offset.checked_rem(stride).unwrap_or(0);
            if within
                .checked_add(target_size)
                .is_none_or(|end| end > stride)
            {
                return Err(unsupported_view(range));
            }
            let field = Arc::make_mut(fields)
                .get_mut(index as usize)
                .ok_or_else(|| unsupported_view(range))?;
            return self.write_memory(field, element, within, target, value, range, depth + 1);
        }
        let index = self.memory_fields(ty, 0, range)?;
        let end = offset
            .checked_add(target_size)
            .ok_or_else(|| unsupported_view(range))?;
        let next = index.partition_point(|field| field.at <= offset);
        if let Some(field) = next.checked_sub(1).and_then(|i| index.get(i))
            && end <= field.end
        {
            return self.write_memory(
                &mut Arc::make_mut(fields)[field.index as usize],
                field.ty,
                offset - field.at,
                target,
                value,
                range,
                depth + 1,
            );
        }
        Err(error(
            range,
            "constant pointer write does not select a represented field".into(),
        ))
    }
}
