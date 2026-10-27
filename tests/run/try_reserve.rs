extern crate alloc;

use std::cell::Cell;
use std::collections::TryReserveError;

struct Owner<'a>(&'a Cell<usize>, usize);
impl Drop for Owner<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn error_contract(error: TryReserveError) {
    let _: &alloc::collections::TryReserveError = &error;
    assert_eq!(error, error.clone());
    assert_eq!(
        format!("{error:?}"),
        "TryReserveError { kind: CapacityOverflow }"
    );
    assert_eq!(
        format!("{error}"),
        "memory allocation failed because the computed capacity exceeded the collection's maximum"
    );
    let object: &dyn std::error::Error = &error;
    assert!(object.source().is_none());
}

fn main() {
    for align in [1, 8, 128] {
        let layout = std::alloc::Layout::from_size_align(17, align).unwrap();
        println!("{layout:?}\n{layout:#?}");
    }
    let drops = Cell::new(0);
    let mut values = Vec::new();
    values.try_reserve_exact(2).unwrap();
    assert_eq!(values.capacity(), 2);
    values.push(Owner(&drops, 11));
    values.push(Owner(&drops, 23));
    let pointer = values.as_ptr();
    let capacity = values.capacity();
    error_contract(values.try_reserve(usize::MAX).unwrap_err());
    error_contract(values.try_reserve_exact(usize::MAX).unwrap_err());
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values.capacity(), capacity);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].1 + values[1].1, 34);
    assert_eq!(drops.get(), 0);
    // This valid layout exceeds macOS arm64 virtual address space. malloc
    // rejects it without consuming physical memory.
    let allocation_error = values
        .try_reserve_exact(isize::MAX as usize / std::mem::size_of::<Owner>() - 2)
        .unwrap_err();
    println!("{allocation_error:?}\n{allocation_error:#?}");
    assert_eq!(allocation_error, allocation_error.clone());
    assert_eq!(
        format!("{allocation_error}"),
        "memory allocation failed because the memory allocator returned an error"
    );
    assert_eq!(values.as_ptr(), pointer);
    assert_eq!(values.capacity(), capacity);
    assert_eq!(values.len(), 2);
    assert_eq!(values[0].1 + values[1].1, 34);
    assert_eq!(drops.get(), 0);
    values.try_reserve(1).unwrap();
    assert!(values.capacity() >= 3);
    assert_eq!(values[1].1, 23);
    drop(values);
    assert_eq!(drops.get(), 2);

    let mut bytes: Vec<u8> = Vec::new();
    bytes.try_reserve(1).unwrap();
    assert_eq!(bytes.capacity(), 8);
    let mut words: Vec<u64> = Vec::new();
    error_contract(
        words
            .try_reserve_exact(isize::MAX as usize / 8 + 1)
            .unwrap_err(),
    );
    assert_eq!(words.capacity(), 0);
    assert!(words.is_empty());
    let mut zeroes: Vec<()> = Vec::new();
    zeroes.try_reserve_exact(usize::MAX).unwrap();
    assert_eq!(zeroes.capacity(), usize::MAX);
    zeroes.push(());
    error_contract(zeroes.try_reserve(usize::MAX).unwrap_err());
    assert_eq!(zeroes.len(), 1);
    let mut grown = Vec::new();
    for i in 0..129 {
        grown.push(i * 3);
    }
    grown.reserve(200);
    grown.reserve_exact(400);
    assert_eq!(grown.len(), 129);
    assert_eq!(grown[128], 384);
    assert!(grown.capacity() >= 529);
    println!("reserve 34 {} {}", drops.get(), bytes.capacity());
}
