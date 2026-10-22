//! `size_of` and `align_of` match rustc's layouts: struct fields
//! reordered by alignment (not for `#[repr(C)]`), tuples keeping their
//! last element last, enums' tags in the smallest integer (or `repr`'s),
//! widened where the variants' fields leave room, and niches: invalid
//! values of a field that other variants use (`Option<&T>`, `Option<bool>`,
//! `Option<char>`, nested `Option`s, enums whose tags leave values free),
//! with fields of the other variants before or after the niche; and in
//! constants.

#![allow(dead_code)]

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(size: usize, align: usize) {
    unsafe {
        printf(c"%zu %zu\n".as_ptr(), size, align);
    }
}

struct Reordered {
    a: u8,
    b: u32,
    c: u8,
    d: u64,
}

#[repr(C)]
struct InOrder {
    a: u8,
    b: u32,
    c: u8,
    d: u64,
}

struct WithNiche(u8, bool, u32);

enum Unit {
    A,
    B,
    C,
}

enum Wide {
    A = -1,
    B = 300,
}

#[repr(u8)]
enum Repr {
    A(u32),
    B,
}

#[repr(C)]
enum CEnum {
    A(u8),
    B(u64),
}

enum Mixed {
    A(u32),
    B(u64),
    C,
}

enum Jagged {
    Big(u8, &'static u8, u16),
    Small(u8),
    None,
}

enum Ordered {
    X(u128),
    Y(fn(u8)),
    Z(Option<[bool; 3]>),
}

enum Single {
    Only(u16),
}

enum Never {}

const SIZE: usize = size_of::<(u8, u64, u16)>() + align_of::<Reordered>();

macro_rules! report {
    ($($t:ty),* $(,)?) => {
        $(show(size_of::<$t>(), align_of::<$t>());)*
    };
}

fn main() {
    report!(
        u8, u16, u32, u64, u128, i128, bool, char, (), &u8, &[u8], &str, *const u8,
        fn(u8), [u32; 3], [bool; 0],
        Reordered, InOrder, WithNiche, Unit, Wide, Repr, CEnum, Mixed, Jagged, Ordered,
        Single, Never,
        Option<u8>, Option<u32>, Option<u64>, Option<&u8>, Option<&[u8]>, Option<bool>,
        Option<char>, Option<Option<bool>>, Option<Option<Option<char>>>, Option<fn(u8)>,
        Option<Unit>, Option<Wide>, Option<Repr>, Option<CEnum>, Option<Mixed>,
        Option<Jagged>, Option<Option<Jagged>>, Option<Ordered>, Option<Single>,
        Option<WithNiche>, Option<[&u8; 2]>, Option<()>, Option<Never>,
        Result<u8, u32>, Result<(), &u8>, Result<&u8, &u16>, Result<u64, bool>,
        Result<Option<&u8>, u8>, Result<[u8; 3], u16>,
        (u8, u32, u8), (u8, u16, u32, u64), (u64, u8), (bool, &u8, u8),
        (Reordered, u8), ([u16; 3], u8, u32), Option<(u8, &u8)>,
        [Option<&u8>; 3], [Option<bool>; 5], [Jagged; 2],
        Option<core::cell::Cell<bool>>, core::cell::RefCell<u8>,
        Option<core::cell::RefCell<bool>>,
    );
    show(SIZE, size_of::<core::mem::MaybeUninit<Option<&u8>>>());
    show(size_of_val(&[1u16, 2, 3]), align_of_val(&7u64));
}
