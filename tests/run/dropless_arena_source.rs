//! Exercises unchanged production allocation and destruction, not all flux-arena.
#[path = "../../crates/flux-arena/src/dropless.rs"]
mod actual;

use actual::DroplessArena;

#[derive(Clone, Copy)]
#[repr(align(16))]
struct Aligned(u8);

#[derive(Clone, Copy)]
#[repr(align(16))]
struct AlignedZst;

fn main() {
    let arena = DroplessArena::new();
    let first = arena.alloc(17u64);
    let numbers = arena.alloc_slice(&[1u16, 2, 3]);
    let text = arena.alloc_str("fluxλ");
    let mut retained = Vec::new();
    for i in 0..10_000u32 {
        retained.push(arena.alloc(i));
    }
    for (i, value) in retained.iter().enumerate() {
        assert_eq!(**value, i as u32);
    }
    *first += 4;
    numbers[1] = 9;
    assert_eq!(*first, 21);
    assert_eq!(numbers, &[1, 9, 3]);
    assert_eq!(text, "fluxλ");
    println!(
        "retained {} {:?} {} {}",
        first,
        numbers,
        text,
        arena.capacity_bytes()
    );

    for i in 0..100u8 {
        arena.alloc(i);
        let value = arena.alloc(Aligned(i));
        assert_eq!((value as *mut Aligned).addr() % 16, 0);
        assert_eq!(value.0, i);
        let value = arena.alloc(i as u64);
        assert_eq!((value as *mut u64).addr() % 8, 0);
        assert_eq!(*value, i as u64);
    }

    let dedicated = DroplessArena::new();
    let before = dedicated.alloc(1u8);
    let large = vec![7u8; 512 * 1024 + 1];
    let copy = dedicated.alloc_slice(&large);
    assert_eq!(copy, &large[..]);
    let after = dedicated.alloc(2u8);
    assert_eq!((after as *mut u8).addr(), (before as *mut u8).addr() + 1);
    println!(
        "dedicated {} {} {}",
        copy.len(),
        copy[copy.len() - 1],
        dedicated.capacity_bytes()
    );

    let empty = DroplessArena::default();
    empty.alloc(());
    let zst = empty.alloc(AlignedZst);
    assert_eq!((zst as *mut AlignedZst).addr() % 16, 0);
    assert!(empty.alloc_slice::<u32>(&[]).is_empty());
    assert_eq!(empty.alloc_slice(&[AlignedZst; 17]).len(), 17);
    assert_eq!(empty.alloc_str(""), "");
    assert_eq!(empty.capacity_bytes(), 0);
    println!("empty {}", empty.capacity_bytes());

    // Each scope runs the actual arena's Drop, including its boxed chunks.
    for i in 0..32u64 {
        let arena = DroplessArena::new();
        let value = arena.alloc(i);
        assert_eq!(*value, i);
        assert_eq!(arena.capacity_bytes(), 4096);
    }
    println!("drops complete");
}
