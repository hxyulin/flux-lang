//! Constant allocation identities and bounded, shared value graphs.

use crate::eval::Value;
use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, MAX_NESTING_DEPTH};
use std::sync::Arc;

const VALUE: u8 = 1;
const FIELDS: u8 = 2;

fn value_key(value: &Arc<Value>) -> usize {
    Arc::as_ptr(value) as usize
}
fn fields_key(fields: &Arc<[Value]>) -> usize {
    Arc::as_ptr(fields) as *const Value as usize
}
fn height(level: u32, depth: u32) -> Result<u32, Error> {
    if depth == 0
        || level
            .checked_add(depth)
            .is_none_or(|n| n > MAX_NESTING_DEPTH)
    {
        Err(Error::new("constant allocation nesting limit exceeded"))
    } else {
        Ok(depth)
    }
}

pub(super) fn value_height(value: &Value, e: &mut Encoder, level: u32) -> Result<u32, Error> {
    height(level, 1)?;
    if let Some(depth) = e.allocation_depth(VALUE, value as *const Value as usize) {
        return height(level, depth);
    }
    let extra = match value {
        Value::Ref(value)
        | Value::InteriorRef(value)
        | Value::DynPointer { pointer: value, .. }
        | Value::PointerView { pointer: value, .. }
        | Value::PointerOffset { pointer: value, .. } => arc_height(value, e, level + 1)?,
        Value::Aggregate(fields) | Value::Variant { fields, .. } => {
            fields_height(fields, e, level + 1)?
        }
        Value::StaticPart { path, .. } => {
            if path.is_empty() {
                1
            } else {
                2
            }
        }
        Value::Function(_) => 1,
        Value::LocalRef { .. } => return Err(Error::new("frame-local value cannot be serialized")),
        _ => 0,
    };
    height(level, 1 + extra)
}
fn arc_height(value: &Arc<Value>, e: &mut Encoder, level: u32) -> Result<u32, Error> {
    let raw = value_key(value);
    if let Some(depth) = e.allocation_depth(VALUE, raw) {
        return height(level, depth);
    }
    e.retain_allocation(VALUE, raw, value.clone())?;
    let depth = value_height(value, e, level)?;
    e.set_allocation_depth(VALUE, raw, value.clone(), depth)?;
    Ok(depth)
}
fn fields_height(fields: &Arc<[Value]>, e: &mut Encoder, level: u32) -> Result<u32, Error> {
    let raw = fields_key(fields);
    if let Some(depth) = e.allocation_depth(FIELDS, raw) {
        return height(level, depth);
    }
    height(level, 1)?;
    e.retain_allocation(FIELDS, raw, fields.clone())?;
    let mut deepest = 0;
    for value in fields.iter() {
        deepest = deepest.max(value_height(value, e, level + 1)?);
    }
    let depth = height(level, 1 + deepest)?;
    e.set_allocation_depth(FIELDS, raw, fields.clone(), depth)?;
    Ok(depth)
}

pub(super) fn encode_value(value: &Arc<Value>, e: &mut Encoder) -> Result<(), Error> {
    let raw = value_key(value);
    if let Some((wire, _)) = e.allocation_reference(VALUE, raw)? {
        0u8.encode(e)?;
        return wire.encode(e);
    }
    let depth = arc_height(value, e, 0)?;
    let wire = e.reserve_allocation(VALUE, raw, value.clone())?;
    1u8.encode(e)?;
    wire.encode(e)?;
    value.as_ref().encode(e)?;
    e.complete_allocation(VALUE, raw, depth)
}
pub(super) fn encode_fields(fields: &Arc<[Value]>, e: &mut Encoder) -> Result<(), Error> {
    let raw = fields_key(fields);
    if let Some((wire, _)) = e.allocation_reference(FIELDS, raw)? {
        0u8.encode(e)?;
        return wire.encode(e);
    }
    let depth = fields_height(fields, e, 0)?;
    let wire = e.reserve_allocation(FIELDS, raw, fields.clone())?;
    1u8.encode(e)?;
    wire.encode(e)?;
    fields.as_ref().encode(e)?;
    e.complete_allocation(FIELDS, raw, depth)
}

pub(super) fn decoded_height(value: &Value, d: &Decoder<'_>) -> Result<u32, Error> {
    if let Some(depth) = d.allocation_depth(VALUE, value as *const Value as usize) {
        return height(0, depth);
    }
    let extra = match value {
        Value::Ref(value)
        | Value::InteriorRef(value)
        | Value::DynPointer { pointer: value, .. }
        | Value::PointerView { pointer: value, .. }
        | Value::PointerOffset { pointer: value, .. } => d
            .allocation_depth(VALUE, value_key(value))
            .ok_or(Error::new("missing constant allocation depth"))?,
        Value::Aggregate(fields) | Value::Variant { fields, .. } => d
            .allocation_depth(FIELDS, fields_key(fields))
            .ok_or(Error::new("missing constant fields depth"))?,
        Value::StaticPart { path, .. } => {
            if path.is_empty() {
                1
            } else {
                2
            }
        }
        Value::Function(_) => 1,
        Value::LocalRef { .. } => return Err(Error::new("frame-local allocation metadata")),
        _ => 0,
    };
    height(0, 1 + extra)
}
pub(super) fn decode_value(d: &mut Decoder<'_>) -> Result<Arc<Value>, Error> {
    let tag = u8::decode(d)?;
    let wire = u32::decode(d)?;
    if tag == 0 {
        return d
            .allocation_reference::<Arc<Value>>(VALUE, wire)
            .map(|(owner, _)| owner);
    }
    if tag != 1 {
        return Err(Error::new("invalid constant allocation tag"));
    }
    d.reserve_allocation(VALUE, wire)?;
    let value = Value::decode(d)?;
    let depth = decoded_height(&value, d)?;
    let owner = Arc::new(value);
    d.set_allocation_depth(VALUE, value_key(&owner), owner.clone(), depth)?;
    d.complete_allocation(VALUE, wire, owner.clone(), depth)?;
    Ok(owner)
}
pub(super) fn decode_fields(d: &mut Decoder<'_>) -> Result<Arc<[Value]>, Error> {
    let tag = u8::decode(d)?;
    let wire = u32::decode(d)?;
    if tag == 0 {
        return d
            .allocation_reference::<Arc<[Value]>>(FIELDS, wire)
            .map(|(owner, _)| owner);
    }
    if tag != 1 {
        return Err(Error::new("invalid constant fields tag"));
    }
    d.reserve_allocation(FIELDS, wire)?;
    let fields: Arc<[Value]> = Vec::<Value>::decode(d)?.into();
    let mut deepest = 0;
    for value in fields.iter() {
        deepest = deepest.max(decoded_height(value, d)?);
    }
    let depth = height(0, 1 + deepest)?;
    d.set_allocation_depth(FIELDS, fields_key(&fields), fields.clone(), depth)?;
    d.complete_allocation(FIELDS, wire, fields.clone(), depth)?;
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dag(levels: usize) -> Value {
        let mut shared = Arc::new(Value::Bool(true));
        for _ in 0..levels {
            shared = Arc::new(Value::Aggregate(Arc::from([
                Value::Ref(shared.clone()),
                Value::Ref(shared),
            ])));
        }
        Value::Ref(shared)
    }
    fn encoded(value: &Value) -> Vec<u8> {
        let mut e = Encoder::new();
        value.encode(&mut e).unwrap();
        e.into_bytes()
    }
    fn check_sharing(mut value: &Value, levels: usize) {
        for _ in 0..levels {
            let Value::Ref(inner) = value else {
                panic!("expected shared reference")
            };
            let Value::Aggregate(fields) = inner.as_ref() else {
                panic!("expected aggregate")
            };
            let [Value::Ref(a), Value::Ref(b)] = fields.as_ref() else {
                panic!("expected shared children")
            };
            assert!(Arc::ptr_eq(a, b));
            value = &fields[0];
        }
        let Value::Ref(inner) = value else {
            panic!("expected leaf reference")
        };
        assert_eq!(inner.as_ref(), &Value::Bool(true));
    }
    #[test]
    fn shared_constant_graphs_encode_linearly_and_keep_allocation_identity() {
        let small = encoded(&dag(20));
        let large = encoded(&dag(40));
        assert!(
            large.len() < small.len() * 3,
            "{} -> {}",
            small.len(),
            large.len()
        );
        assert!(large.len() < 4096, "{} bytes", large.len());
        let mut d = Decoder::new(&large);
        let copy = Value::decode(&mut d).unwrap();
        d.finish().unwrap();
        check_sharing(&copy, 40);
        assert_eq!(large, encoded(&copy));
    }
    #[test]
    fn invalid_allocation_references_and_cycles_are_rejected() {
        let mut reference = Encoder::new();
        10u8.encode(&mut reference).unwrap();
        0u8.encode(&mut reference).unwrap();
        0u32.encode(&mut reference).unwrap();
        assert!(Value::decode(&mut Decoder::new(&reference.into_bytes())).is_err());
        let mut cycle = Encoder::new();
        // Ref -> definition 0 -> Ref -> reference to pending definition 0.
        10u8.encode(&mut cycle).unwrap();
        1u8.encode(&mut cycle).unwrap();
        0u32.encode(&mut cycle).unwrap();
        10u8.encode(&mut cycle).unwrap();
        0u8.encode(&mut cycle).unwrap();
        0u32.encode(&mut cycle).unwrap();
        assert!(Value::decode(&mut Decoder::new(&cycle.into_bytes())).is_err());
    }
    #[test]
    fn semantic_depth_cannot_be_hidden_behind_previously_completed_allocations() {
        let mut e = Encoder::new();
        let mut shared = Arc::new(Value::Bool(true));
        for _ in 0..flux_hir::flib::MAX_NESTING_DEPTH - 2 {
            Value::Ref(shared.clone()).encode(&mut e).unwrap();
            shared = Arc::new(Value::Ref(shared));
        }
        Value::Ref(shared.clone()).encode(&mut e).unwrap();
        shared = Arc::new(Value::Ref(shared));
        assert!(Value::Ref(shared).encode(&mut e).is_err());
        let bytes = e.into_bytes();
        let mut d = Decoder::new(&bytes);
        for _ in 0..flux_hir::flib::MAX_NESTING_DEPTH - 1 {
            Value::decode(&mut d).unwrap();
        }
        d.finish().unwrap();
    }
    #[test]
    fn encoded_allocation_owners_survive_until_the_encoder_is_dropped() {
        let shared = Arc::new(Value::Bool(true));
        let weak = Arc::downgrade(&shared);
        let mut e = Encoder::new();
        Value::Ref(shared).encode(&mut e).unwrap();
        assert!(weak.upgrade().is_some());
        drop(e);
        assert!(weak.upgrade().is_none());
    }
}
