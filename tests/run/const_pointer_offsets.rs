use std::ptr;
#[repr(C, align(64))]
struct Block {
    prefix: u64,
    words: [u32; 6],
    tail: u64,
}
static BLOCK: Block = Block {
    prefix: 91,
    words: [7, 11, 13, 17, 19, 23],
    tail: 97,
};
const BASE: *const u32 = BLOCK.words.as_ptr();
const MIDDLE: *const u32 = unsafe { BASE.add(2) };
const END: *const u32 = unsafe { BASE.add(6) };
const LAST: u32 = unsafe { *END.offset(-1) };
const READ: &u32 = unsafe { &*MIDDLE };
const TAIL: &[u32] = unsafe { std::slice::from_raw_parts(MIDDLE, 4) };
const EMPTY: &[u32] = unsafe { std::slice::from_raw_parts(END, 0) };
struct Pointers {
    first: *const u32,
    second: *const u32,
    tail: &'static [u32],
}
// SAFETY: these immutable pointers address the immutable BLOCK allocation.
unsafe impl Sync for Pointers {}
static POINTERS: Pointers = Pointers {
    first: MIDDLE,
    second: END,
    tail: TAIL,
};
const fn split(buf: &[u32]) -> (&[u32; 2], &[u32]) {
    assert!(buf.len() >= 2);
    let p = buf.as_ptr();
    // SAFETY: the first two elements and the remaining slice lie in buf.
    unsafe {
        (
            &*p.cast::<[u32; 2]>(),
            std::slice::from_raw_parts(p.add(2), buf.len() - 2),
        )
    }
}
const SPLIT: (&[u32; 2], &[u32]) = split(&BLOCK.words);
const SLICE: &[u32] = match &[31, 37, 41, 43] {
    [_, rest @ ..] => rest,
};
const REWIND: u32 = unsafe { *SLICE.as_ptr().sub(1) };
static mut MUTABLE: [u32; 3] = [47, 53, 59];
const MUT: *mut u32 = unsafe { (&raw mut MUTABLE).cast::<u32>().add(1) };
fn identity<T>(x: T) -> T {
    x
}
fn increment(x: u32) -> u32 {
    x + 1
}
static FUNCTIONS: [fn(u32) -> u32; 2] = [increment, identity::<u32>];
const SELECTED: fn(u32) -> u32 = unsafe { *FUNCTIONS.as_ptr().add(1) };
const NONNULL: ptr::NonNull<u32> = unsafe { ptr::NonNull::new_unchecked(BASE as *mut u32).add(2) };
fn main() {
    assert_eq!(NONNULL.as_ptr() as *const u32, MIDDLE);
    assert_eq!(SELECTED(89), 89);
    assert_eq!(LAST, 23);
    assert_eq!(*READ, 13);
    assert_eq!(TAIL, &[13, 17, 19, 23]);
    assert!(EMPTY.is_empty());
    assert_eq!(MIDDLE, unsafe { BLOCK.words.as_ptr().add(2) });
    assert_eq!(POINTERS.first, MIDDLE);
    assert_eq!(POINTERS.second, END);
    assert_eq!(POINTERS.tail.as_ptr(), MIDDLE);
    assert_eq!(TAIL.as_ptr(), MIDDLE);
    assert_eq!(EMPTY.as_ptr(), END);
    assert!(ptr::eq(SPLIT.0.as_ptr(), BLOCK.words.as_ptr()));
    assert_eq!(SPLIT.1.as_ptr(), MIDDLE);
    assert_eq!(SPLIT.1, TAIL);
    assert_eq!(REWIND, 31);
    unsafe {
        *MUT = 61;
        assert_eq!((*(&raw const MUTABLE))[1], 61);
    }
    let mut data = [67u32, 71, 73, 79];
    let shared_add: unsafe fn(*const u32, usize) -> *const u32 = <*const u32>::add;
    let mutable_add: unsafe fn(*mut u32, usize) -> *mut u32 = <*mut u32>::add;
    let negative: unsafe fn(*const u32, isize) -> *const u32 = <*const u32>::offset;
    unsafe {
        assert_eq!(*shared_add(data.as_ptr(), 2), 73);
        *mutable_add(data.as_mut_ptr(), 1) = 83;
        assert_eq!(*negative(data.as_ptr().add(3), -2), 83);
        assert_eq!(*data.as_mut_ptr().add(3).sub(1), 73);
    }
    assert_eq!(data, [67, 83, 73, 79]);
    println!("{} {} {} {}", LAST, *READ, REWIND, data[1]);
}
