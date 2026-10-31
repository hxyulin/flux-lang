use std::borrow::{Borrow, BorrowMut};
fn borrow_array<T, const N: usize>(array: &[T; N]) -> &[T] {
    array.borrow()
}
static DATA: [u32; 4] = [11, 22, 33, 44];
const RAW_SLICE: *const [u32] = core::ptr::slice_from_raw_parts(DATA.as_ptr(), 3);
const SHARED: &[u32] = unsafe { core::slice::from_raw_parts(DATA.as_ptr(), 3) };
const UNINIT_VALUE: u32 = unsafe { core::mem::MaybeUninit::new(17).assume_init() };
const PROMOTED: &[u8] = unsafe { core::slice::from_raw_parts([2u8, 3, 5].as_ptr(), 2) };
fn generic_get<C: core::ops::Index<usize, Output = u32>>(value: &C, index: usize) -> u32 {
    value[index]
}
fn generic_set<C: core::ops::IndexMut<usize, Output = u32>>(
    value: &mut C,
    index: usize,
    item: u32,
) {
    value[index] = item;
}
fn generic_range<C: core::ops::Index<core::ops::Range<usize>, Output = [u32]>>(
    value: &C,
) -> &[u32] {
    &value[1..3]
}
fn borrowed_arrays() {
    let mut words = [3u32, 5, 8, 13];
    assert_eq!(generic_get(&words, 2), 8);
    generic_set(&mut words, 2, 9);
    assert_eq!(generic_range(&words), &[5, 9]);
    let original = words.as_ptr();
    let array: &[u32; 4] = words.as_slice().try_into().unwrap();
    assert_eq!(array.as_ptr(), original);
    let array: &mut [u32; 4] = words.as_mut_slice().try_into().unwrap();
    array[3] = 21;
    assert_eq!(words[3], 21);
    assert!(<&[u32; 3]>::try_from(words.as_slice()).is_err());
    assert!(<&mut [u32; 3]>::try_from(words.as_mut_slice()).is_err());
    let owned = [String::from("owned"), String::from("borrowed")];
    let view: &[String; 2] = owned.as_slice().try_into().unwrap();
    assert_eq!(view[0], "owned");
    assert_eq!(view.as_ptr(), owned.as_ptr());
    let empty: &[String; 0] = <&[String; 0]>::try_from(&owned[..0]).unwrap();
    assert!(empty.is_empty());
    let zsts = [(); 4];
    let zview: &[(); 4] = zsts.as_slice().try_into().unwrap();
    assert_eq!(zview.len(), 4);
}
fn main() {
    borrowed_arrays();
    assert_eq!(unsafe { &*RAW_SLICE }, &[11, 22, 33]);
    assert_eq!(SHARED, &[11, 22, 33]);
    assert_eq!(SHARED.as_ptr(), DATA.as_ptr());
    assert_eq!(UNINIT_VALUE, 17);
    assert_eq!(PROMOTED, &[2, 3]);
    let mut values = [10u32, 20, 30, 40, 50];
    assert_eq!(borrow_array(&values), &[10, 20, 30, 40, 50]);
    let slice: &mut [u32] = values.borrow_mut();
    slice[0] = 11;
    let base = values.as_ptr();
    for count in [-17isize, -1, 0, 1, 3, 17, isize::MIN, isize::MAX] {
        assert_eq!(
            base.wrapping_offset(count)
                .wrapping_offset(count.wrapping_neg()),
            base
        );
        assert_eq!(
            base.wrapping_offset(count) as usize,
            (base as usize).wrapping_add((count as usize).wrapping_mul(4))
        );
    }
    let ptr = values.as_mut_ptr();
    unsafe {
        *ptr.wrapping_offset(2) += 1;
    }
    let (chunks, tail) = values.as_chunks::<2>();
    assert_eq!(chunks, &[[11, 20], [31, 40]]);
    assert_eq!(tail, &[50]);
    assert_eq!(chunks.as_ptr() as *const u32, values.as_ptr());
    let (chunks, tail) = values.as_chunks_mut::<2>();
    chunks[0][1] += 2;
    tail[0] += 3;
    assert_eq!(values, [11, 22, 31, 40, 53]);
    assert_eq!([1u8].as_chunks::<8>(), (&[][..], &[1][..]));
    let empty: [u8; 0] = [];
    assert_eq!(empty.as_chunks::<3>(), (&[][..], &[][..]));
    let zsts = [(); 7];
    let (groups, rest) = zsts.as_chunks::<3>();
    assert_eq!((groups.len(), rest.len()), (2, 1));
    for start in 0..values.len() {
        for end in start..=values.len() {
            let slice = &values[start..end];
            let (rest, groups) = slice.as_rchunks::<2>();
            assert_eq!(rest, &slice[..slice.len() % 2]);
            assert_eq!(groups.len(), slice.len() / 2);
            assert_eq!(
                groups.as_ptr() as *const u32,
                slice[slice.len() % 2..].as_ptr()
            );
            for i in 0..groups.len() {
                assert_eq!(
                    groups[i],
                    [slice[rest.len() + i * 2], slice[rest.len() + i * 2 + 1]]
                );
            }
        }
    }
    assert_eq!([1u8].as_rchunks::<8>(), (&[1][..], &[][..]));
    assert_eq!(empty.as_rchunks::<3>(), (&[][..], &[][..]));
    let (prefix, triples) = zsts.as_rchunks::<3>();
    assert_eq!((prefix.len(), triples.len()), (1, 2));
    let owned = [String::from("a"), String::from("b"), String::from("c")];
    let (pairs, rest) = owned.as_chunks::<2>();
    assert_eq!(pairs[0][1], "b");
    assert_eq!(rest[0], "c");
    let (rest, pairs) = owned.as_rchunks::<2>();
    assert_eq!(rest[0], "a");
    assert_eq!(pairs[0][0], "b");
    assert_eq!(pairs[0][1], "c");
    println!("pointer array {} {}", values[2], groups.len());
}
