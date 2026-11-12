//! Raw storage copies preserve typed payloads, uninitialized slots and addresses.
use super::*;

impl Evaluator<'_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn copy_storage(
        &mut self,
        source: Value,
        destination: Value,
        element: Ty,
        count: u64,
        operation: flux_hir::Intrinsic,
        range: TextRange,
    ) -> Result<(), EvalError> {
        let element_layout = self.view_layout(element, range)?;
        if !element_layout.exact {
            return Err(error(
                range,
                "constant copy element layout is not supported yet".into(),
            ));
        }
        let bytes = count
            .checked_mul(element_layout.size)
            .filter(|&n| n <= i64::MAX as u64)
            .ok_or_else(|| error(range, "constant copy size overflows isize".into()))?;
        let source_ty = Ty::raw_ptr(flux_ty::Mutability::Not, element);
        let destination_ty = Ty::raw_ptr(flux_ty::Mutability::Mut, element);
        let (source, source_ty, source_offset) = self.pointer_origin(source, source_ty, range)?;
        let (destination, destination_ty, destination_offset) =
            self.pointer_origin(destination, destination_ty, range)?;
        // Even empty copies require non-null, aligned pointers. They do not
        // access storage, so aligned dangling addresses are otherwise allowed.
        for (pointer, pointer_ty, offset) in [
            (&source, source_ty, source_offset),
            (&destination, destination_ty, destination_offset),
        ] {
            if let Value::RawAddr(address) = pointer {
                if *address == 0 || address % element_layout.align != 0 {
                    return Err(error(
                        range,
                        "constant copy requires non-null aligned pointers".into(),
                    ));
                }
                if bytes != 0 {
                    return Err(error(range, "constant copy requires an allocation".into()));
                }
            } else {
                let ty = self.allocation_type(pointer, pointer_ty, range)?;
                let layout = self.view_layout(ty, range)?;
                if !layout.exact
                    || layout.align < element_layout.align
                    || offset % element_layout.align != 0
                {
                    return Err(error(range, "unaligned constant copy".into()));
                }
                if offset
                    .checked_add(bytes)
                    .is_none_or(|end| end > layout.size)
                {
                    return Err(error(range, "constant copy is out of bounds".into()));
                }
            }
        }
        if bytes == 0 {
            return Ok(());
        }
        let Value::FramePointer {
            origin,
            writable: true,
            ..
        } = destination
        else {
            return Err(error(
                range,
                "constant copies require mutable local storage".into(),
            ));
        };
        let allocation = self.frame_allocation(origin, range)?;
        if allocation.frozen || !origin.mutable {
            return Err(error(
                range,
                "constant copies require mutable local storage".into(),
            ));
        }
        // Shared snapshots of projected locals retain the root token but not
        // their root-relative offset. Do not infer nonoverlap from that view's
        // offset until the evaluator has an allocation-relative address.
        if operation == flux_hir::Intrinsic::CopyNonoverlapping
            && matches!(&source, Value::LocalRef { origin: other, .. } if other.token == origin.token)
            && pointer_pointee(source_ty) != Some(allocation.ty)
        {
            return Err(error(
                range,
                "nonoverlap of projected local snapshots in constants is not supported yet".into(),
            ));
        }
        if operation == flux_hir::Intrinsic::CopyNonoverlapping
            && matches!(&source, Value::FramePointer { origin: other, .. } | Value::LocalRef { origin: other, .. } if other.token == origin.token)
            && source_offset < destination_offset + bytes
            && destination_offset < source_offset + bytes
        {
            return Err(error(
                range,
                "overlapping constant copy_nonoverlapping".into(),
            ));
        }
        let stored_ty = self.allocation_type(&source, source_ty, range)?;
        // Snapshot before the first write: copy permits overlap in either direction.
        let stored = match &source {
            Value::FramePointer { origin, .. } => {
                self.frame_allocation(*origin, range)?.value.clone()
            }
            _ => self.pointer_value(&source, source_ty, source_ty, range)?,
        };
        self.charge_view(count, range)?;
        let mut values = Vec::with_capacity(count as usize);
        for i in 0..count {
            values.push(self.copy_value(
                &stored,
                stored_ty,
                source_offset + i * element_layout.size,
                element,
                range,
                0,
            )?);
        }
        let mut allocation = self
            .frame_memory
            .remove(&origin.token)
            .expect("validated destination allocation");
        let result = (|| {
            for (i, value) in values.into_iter().enumerate() {
                self.write_memory(
                    &mut allocation.value,
                    allocation.ty,
                    destination_offset + i as u64 * element_layout.size,
                    element,
                    value,
                    range,
                    0,
                )?;
            }
            Ok(())
        })();
        self.frame_memory.insert(origin.token, allocation);
        if let Some(version) = self.snapshot_versions.get_mut(&origin.token) {
            *version += 1;
        }
        result
    }

    /// Unlike a typed read, a raw copy does not require initialized payloads.
    #[allow(clippy::too_many_arguments)]
    fn copy_value(
        &mut self,
        value: &Value,
        from: Ty,
        offset: u64,
        to: Ty,
        range: TextRange,
        depth: u32,
    ) -> Result<Value, EvalError> {
        if depth > DEPTH_LIMIT {
            return Err(unsupported_view(range));
        }
        let from = flux_solve::traits::normalize(self.items, self.sigs, &[], from);
        let to = flux_solve::traits::normalize(self.items, self.sigs, &[], to);
        if offset == 0 && from == to {
            return Ok(value.clone());
        }
        if matches!(value, Value::Uninit(_)) {
            return Ok(Value::Uninit(to));
        }
        let size = self.view_layout(to, range)?.size;
        if let Value::Aggregate(fields) = value {
            if let TyKind::Array(element, _) = from.kind() {
                let stride = self.view_layout(element, range)?.size;
                if stride != 0
                    && size <= stride - offset % stride
                    && let Some(value) = fields.get((offset / stride) as usize)
                {
                    return self.copy_value(value, element, offset % stride, to, range, depth + 1);
                }
            } else {
                let index = self.memory_fields(from, 0, range)?;
                let next = index.partition_point(|field| field.at <= offset);
                if let Some(field) = next.checked_sub(1).and_then(|i| index.get(i))
                    && offset + size <= field.end
                    && let Some(value) = fields.get(field.index as usize)
                {
                    return self.copy_value(
                        value,
                        field.ty,
                        offset - field.at,
                        to,
                        range,
                        depth + 1,
                    );
                }
            }
        }
        // Reinterpret initialized scalar bytes using the existing validity and
        // relocation checks. Padding and partial pointer representations fail.
        self.memory_value(value, from, offset, to, range, depth + 1)
    }
}
