use std::ptr::NonNull;

#[repr(align(64))]
struct Aligned(u64);
#[repr(align(128))]
struct Zst;

const fn empty<T>() -> Vec<T> { Vec::new() }
const WORDS: Vec<u32> = empty();
const ALIGNED: Vec<Aligned> = empty();
const ZERO: Vec<Zst> = empty();
const RAW: *const u32 = 64usize as *const u32;
const SIGNED: *mut u8 = -1i8 as *mut u8;
const WIDE_INT: *const u8 = u128::MAX as *const u8;
const NONNULL: Option<NonNull<Aligned>> = Some(NonNull::dangling());

struct Header { pointer: *const u32, other: *const u8 }
// These raw addresses are never dereferenced; the shared static is immutable.
unsafe impl Sync for Header {}
static HEADER: Header = Header { pointer: RAW, other: 128usize as *const u8 };

struct Arena<T> { data: Vec<T> }
impl<T> Arena<T> {
    const fn new() -> Self { Self { data: Vec::new() } }
}
const ARENA: Arena<String> = Arena::new();

fn main() {
    let mut words = WORDS;
    assert_eq!(words.len(), 0);
    assert_eq!(words.capacity(), 0);
    assert_eq!(words.as_ptr() as usize, 4);
    words.push(7);
    words.push(11);
    println!("words {:?} {}", words, words.capacity());
    let mut aligned = ALIGNED;
    assert_eq!(aligned.as_ptr() as usize, 64);
    aligned.push(Aligned(23));
    assert_eq!(aligned.as_ptr() as usize % 64, 0);
    println!("aligned {} {}", aligned[0].0, aligned.len());
    let mut zsts = ZERO;
    assert_eq!(zsts.capacity(), usize::MAX);
    assert_eq!(zsts.as_ptr() as usize, 128);
    zsts.push(Zst);
    println!("zst {} {}", zsts.len(), zsts.capacity());
    assert_eq!(RAW as usize, 64);
    let reference_to_pointer = &RAW;
    assert_eq!(*reference_to_pointer as usize, 64);
    assert_eq!(SIGNED as usize, usize::MAX);
    assert_eq!(WIDE_INT as usize, usize::MAX);
    assert_eq!(HEADER.pointer as usize, 64);
    assert_eq!(HEADER.other as usize, 128);
    assert_eq!(NONNULL.unwrap().as_ptr() as usize, 64);
    let pointers = &[RAW, 256usize as *const u32];
    println!("pointers {} {} {}", pointers[0] as usize, pointers[1] as usize, SIGNED as usize);
    let mut arena = ARENA;
    arena.data.push(String::from("real"));
    println!("arena {}", arena.data[0]);
    // Each use of a constant creates a separate empty owner.
    let mut second = WORDS;
    second.push(19);
    assert_eq!(words, [7, 11]);
    assert_eq!(second, [19]);
}
