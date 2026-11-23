//! Length-prefixed little-endian storage for homogeneous integer tables.

use flux_hir::flib::{Decode, Decoder, Encode, Encoder, Error, MAX_RECORD_LENGTH};
use flux_ty::IntTy;

const MIN_TABLE_LEN: usize = 32;

pub(super) fn table_type<T>(
    entries: &[T],
    integer: impl Fn(&T) -> Option<(u128, IntTy)>,
) -> Option<IntTy> {
    if !(MIN_TABLE_LEN..=MAX_RECORD_LENGTH).contains(&entries.len()) {
        return None;
    }
    let (_, ty) = integer(&entries[0])?;
    let width = ty.bits();
    if width > 64 {
        return None;
    }
    entries
        .iter()
        .all(|entry| {
            integer(entry).is_some_and(|(bits, actual)| actual == ty && bits >> width == 0)
        })
        .then_some(ty)
}

pub(super) fn encode<T>(
    entries: &[T],
    ty: IntTy,
    integer: impl Fn(&T) -> Option<(u128, IntTy)>,
    e: &mut Encoder,
) -> Result<(), Error> {
    let width = (ty.bits() / 8) as usize;
    ty.encode(e)?;
    ((entries.len() * width) as u32).encode(e)?;
    for entry in entries {
        let (bits, _) = integer(entry).expect("entries were checked by table_type");
        e.write(&bits.to_le_bytes()[..width]);
    }
    Ok(())
}

pub(super) fn reserve<T>(e: &mut Encoder, entries: &[T]) -> bool {
    // The ordinary encoding remains available when the packed budget is full.
    e.charge_integer_table_expansion(entries.len(), std::mem::size_of::<T>())
        .is_ok()
}

pub(super) fn decode<T>(
    d: &mut Decoder<'_>,
    wrap: impl Fn(u128, IntTy) -> T,
) -> Result<(IntTy, Vec<T>), Error> {
    let ty = IntTy::decode(d)?;
    let width = (ty.bits() / 8) as usize;
    let bytes = d.blob()?;
    let count = bytes.len() / width;
    if width > 8
        || !bytes.len().is_multiple_of(width)
        || !(MIN_TABLE_LEN..=MAX_RECORD_LENGTH).contains(&count)
    {
        return Err(Error::new("invalid packed integer table"));
    }
    d.charge_integer_table_expansion(count, std::mem::size_of::<T>())?;
    macro_rules! read_lanes {
        ($width:literal, $word:ty) => {
            bytes
                .chunks_exact($width)
                .map(|bytes| {
                    let bits =
                        <$word>::from_le_bytes(bytes.try_into().expect("complete integer lane"));
                    wrap(u128::from(bits), ty)
                })
                .collect()
        };
    }
    let entries = match width {
        1 => bytes
            .iter()
            .map(|&byte| wrap(u128::from(byte), ty))
            .collect(),
        2 => read_lanes!(2, u16),
        4 => read_lanes!(4, u32),
        8 => read_lanes!(8, u64),
        _ => unreachable!("integer width was checked"),
    };
    Ok((ty, entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::Value;
    use crate::{AggregateKind, Constant, Operand, Rvalue};
    use flux_ty::Ty;

    #[test]
    fn packed_integer_operands_preserve_signed_bits() {
        let ty = IntTy::I16;
        let values = [0, 0x7fff, 0x8000, 0xffff];
        let operands = (0..MIN_TABLE_LEN)
            .map(|index| {
                Operand::Const(Constant::Int {
                    bits: values[index % values.len()],
                    ty,
                })
            })
            .collect();
        let original = Rvalue::Aggregate(AggregateKind::Array(Ty::int(ty)), operands);
        let mut encoder = Encoder::new();
        original.encode(&mut encoder).unwrap();
        let bytes = encoder.into_bytes();
        assert_eq!(bytes[0], 15);
        let mut decoder = Decoder::new(&bytes);
        assert_eq!(Rvalue::decode(&mut decoder).unwrap(), original);
        decoder.finish().unwrap();
        assert!(Rvalue::decode(&mut Decoder::new(&bytes[..bytes.len() - 1])).is_err());
    }

    #[test]
    fn packed_integer_values_preserve_allocation_reference() {
        let fields: std::sync::Arc<[Value]> = vec![
            Value::Int {
                bits: 0x1234,
                ty: IntTy::U16,
            };
            MIN_TABLE_LEN
        ]
        .into();
        let mut encoder = Encoder::new();
        super::super::allocations::encode_fields(&fields, &mut encoder).unwrap();
        super::super::allocations::encode_fields(&fields, &mut encoder).unwrap();
        let bytes = encoder.into_bytes();
        assert_eq!(bytes[0], 3);
        let mut decoder = Decoder::new(&bytes);
        let first = super::super::allocations::decode_fields(&mut decoder).unwrap();
        let second = super::super::allocations::decode_fields(&mut decoder).unwrap();
        assert_eq!(first, fields);
        assert!(std::sync::Arc::ptr_eq(&first, &second));
        decoder.finish().unwrap();
    }

    #[test]
    fn packed_integer_tables_reject_partial_elements() {
        let mut encoder = Encoder::new();
        IntTy::I16.encode(&mut encoder).unwrap();
        encoder.blob(&[0; 2 * MIN_TABLE_LEN + 1]).unwrap();
        let bytes = encoder.into_bytes();
        assert!(decode(&mut Decoder::new(&bytes), |bits, _| bits).is_err());
    }
}
