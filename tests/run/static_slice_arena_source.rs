// Exercise the unchanged production arena, including its intentional leaks.
#[path = "../../crates/flux-arena/src/static_slice.rs"]
mod static_slice;
use static_slice::StaticSliceArena;

#[derive(Clone, Copy)]
#[repr(align(64))]
struct Aligned(u64);
#[derive(Clone, Copy)]
#[repr(align(64))]
struct AlignedZst;

const INITIAL: StaticSliceArena<u64> = StaticSliceArena::new();

fn main() {
    let mut retained = Vec::new();
    {
        let mut arena = INITIAL;
        for len in [0, 1, 512, 513, 2048] {
            let mut values = Vec::new();
            for i in 0..len {
                values.push(i as u64 + len as u64);
            }
            let slice = arena.alloc(&values);
            assert_eq!(slice.len(), len);
            assert_eq!(slice, &values[..]);
            retained.push(slice);
        }
    }
    let mut sum = 0u64;
    for slice in &retained {
        for i in 0..slice.len() {
            assert_eq!(slice[i], i as u64 + slice.len() as u64);
            sum += slice[i];
        }
    }
    // A fresh arena starts with a 512-element u64 chunk. Its first two
    // allocations are adjacent; the third needs distinct live storage.
    let (first, next, large);
    {
        let mut arena = StaticSliceArena::default();
        first = arena.alloc(&[1u64]);
        next = arena.alloc(&[2u64]);
        large = arena.alloc(&[3u64; 512]);
        assert_eq!(next.as_ptr(), first.as_ptr().wrapping_add(1));
        let start = large.as_ptr().addr();
        let end = start + large.len() * std::mem::size_of::<u64>();
        assert!(first.as_ptr().addr() < start || first.as_ptr().addr() >= end);
        assert!(next.as_ptr().addr() < start || next.as_ptr().addr() >= end);
    }
    assert_eq!(first[0], 1);
    assert_eq!(next[0], 2);
    assert_eq!(large[511], 3);

    let aligned;
    {
        let mut arena = StaticSliceArena::new();
        aligned = arena.alloc(&[Aligned(17); 513]);
        for len in [0, 1, 512, 513, 2048] {
            let values = vec![Aligned(19); len];
            let slice = arena.alloc(&values);
            assert_eq!(slice.len(), len);
            assert_eq!(slice.as_ptr().addr() % 64, 0);
            for value in slice {
                assert_eq!(value.0, 19);
            }
        }
    }
    assert_eq!(aligned.as_ptr().addr() % 64, 0);
    assert_eq!(aligned[512].0, 17);

    let zst;
    {
        let mut arena = StaticSliceArena::default();
        zst = arena.alloc(&[AlignedZst; 2048]);
        for len in [0, 1, 512, 513, 2048] {
            let values = vec![AlignedZst; len];
            let slice = arena.alloc(&values);
            assert_eq!(slice.len(), len);
            assert_eq!(slice.as_ptr().addr() % 64, 0);
        }
    }
    assert_eq!(zst.len(), 2048);
    assert_eq!(zst.as_ptr().addr() % 64, 0);
    println!(
        "static-slice-source {} {} {}",
        sum,
        aligned[512].0,
        zst.len()
    );
}
