// Slice splitting and swapping, string escapes and case, numeric chars,
// ManuallyDrop, UnsafeCell and Cell views, and atomic defaults and fences.
#![allow(deprecated)]
use std::cell::{Cell, UnsafeCell};
use std::mem::{ManuallyDrop, MaybeUninit};
use std::sync::atomic::{self, AtomicPtr, AtomicU32, AtomicUsize, Ordering};

struct Noisy(&'static str);
impl Drop for Noisy {
    fn drop(&mut self) { println!("drop {}", self.0); }
}

fn main() {
    let bytes = b"one\ntwo\n\nthree";
    let lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
    println!("{:?}", lines);
    let back: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').rev().collect();
    println!("{:?}", back);
    println!("{:?}", b"a,b,c,d".splitn(3, |&b| b == b',').collect::<Vec<_>>());
    println!("{:?}", b"a,b,c,d".rsplitn(2, |&b| b == b',').collect::<Vec<_>>());
    println!("{:?}", b"a,b,,c".rsplit(|&b| b == b',').collect::<Vec<_>>());
    println!("{:?} {:?}", b"file.tar.gz".strip_suffix(b".gz"), b"x".strip_suffix(b"xy"));
    println!("{:?}", [1, 1, 2, 3, 3, 3, 1].chunk_by(|a, b| a == b).collect::<Vec<_>>());
    let mut numbers = [1, 0, 2, 3, 0, 4];
    for piece in numbers.split_mut(|&n| n == 0) { piece.reverse(); }
    println!("{:?}", numbers);
    let mut left = [1, 2, 3];
    let mut right = [7, 8, 9];
    left.swap_with_slice(&mut right);
    let mut counter = 0;
    let mut filled = [0; 4];
    filled.fill_with(|| { counter += 2; counter });
    println!("{:?} {:?} {:?}", left, right, filled);
    if let Some((last, rest)) = filled.split_last_mut() { *last = rest.len(); }
    if let Some((first, _)) = filled.split_first_mut() { *first = 99; }
    println!("{:?} {:?}", filled, b"  pad \t".trim_ascii());

    let text = "tab\there \"q\" \u{301}é\u{301}\u{200b}";
    println!("{} | {} | {}", text.escape_debug(), text.escape_default(), "ß straße ǆ".to_uppercase());
    println!("{:?}", "a 😀".encode_utf16().collect::<Vec<u16>>());
    println!("{:?}", "  split \t ascii\nwords  ".split_ascii_whitespace().collect::<Vec<_>>());
    println!("{:?}", " a b ".split_ascii_whitespace().rev().collect::<Vec<_>>());
    println!("[{}] [{}] [{}]", "xxabxx".trim_left_matches('x'), "xxabxx".trim_right_matches('x'), "  t ".trim_left());
    println!("{:?}", ['7', 'a', '½', '٣', 'Ⅻ', '①', ' '].map(|c| c.is_numeric()));

    let mut slot = ManuallyDrop::new(Noisy("taken"));
    let taken = unsafe { ManuallyDrop::take(&mut slot) };
    drop(taken);
    let mut dropped = ManuallyDrop::new(Noisy("manual"));
    unsafe { ManuallyDrop::drop(&mut dropped) };
    println!("{:?} {:?}", ManuallyDrop::new(5), ManuallyDrop::new(1) < ManuallyDrop::new(2));
    println!("{:#?}", ManuallyDrop::new((1, "x")));
    let cell = UnsafeCell::new(3);
    unsafe { *UnsafeCell::raw_get(&cell) += 1 };
    println!("{:?}", cell);
    println!("{}", cell.into_inner());
    let mut value = 10;
    let view = Cell::from_mut(&mut value);
    view.set(11);
    println!("{} {}", value, std::mem::size_of::<MaybeUninit<u64>>());

    let counter = AtomicUsize::default();
    counter.fetch_add(3, Ordering::Relaxed);
    let pointer: AtomicPtr<u8> = AtomicPtr::default();
    let word = AtomicU32::from(9);
    atomic::compiler_fence(Ordering::SeqCst);
    atomic::spin_loop_hint();
    static LEGACY: AtomicUsize = atomic::ATOMIC_USIZE_INIT;
    LEGACY.fetch_add(1, Ordering::SeqCst);
    println!("{:?} {} {:?} {}", counter, pointer.load(Ordering::Relaxed).is_null(), word, LEGACY.load(Ordering::SeqCst));
}
