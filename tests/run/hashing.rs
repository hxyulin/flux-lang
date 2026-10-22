//! `core::hash`: `Hash` for the integers, `bool`, `char`, `str`, `()`,
//! tuples, arrays, slices, references, `Option`, `Result`, `Ordering`
//! and `Reverse`, and `#[derive(Hash)]` on structs and enums (with and
//! without a `repr`, of one variant and of several), each making the
//! same calls of a `Hasher`'s methods as rustc's, which a hasher that
//! records them shows; `Hasher`'s provided methods; `BuildHasher` with
//! `BuildHasherDefault` and `hash_one`.

use core::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

/// Records each call: the bytes of `write` and the integer writes.
#[derive(Default)]
struct Rec {
    h: u64,
}

impl Hasher for Rec {
    fn finish(&self) -> u64 {
        self.h
    }
    fn write(&mut self, bytes: &[u8]) {
        unsafe { printf(c"  write %d:".as_ptr(), bytes.len() as i32) };
        for &b in bytes {
            unsafe { printf(c" %d".as_ptr(), b as i32) };
            self.h = self.h.wrapping_mul(31).wrapping_add(b as u64);
        }
        unsafe { printf(c"\n".as_ptr()) };
    }
    fn write_u8(&mut self, i: u8) {
        unsafe { printf(c"  u8 %d\n".as_ptr(), i as i32) };
        self.h = self.h.wrapping_mul(37).wrapping_add(i as u64);
    }
    fn write_u32(&mut self, i: u32) {
        unsafe { printf(c"  u32 %u\n".as_ptr(), i) };
        self.h = self.h.wrapping_mul(41).wrapping_add(i as u64);
    }
    fn write_isize(&mut self, i: isize) {
        unsafe { printf(c"  isize %lld\n".as_ptr(), i as i64) };
        self.h = self.h.wrapping_mul(43).wrapping_add(i as u64);
    }
    fn write_usize(&mut self, i: usize) {
        unsafe { printf(c"  usize %llu\n".as_ptr(), i as u64) };
        self.h = self.h.wrapping_mul(47).wrapping_add(i as u64);
    }
}

/// Only `write`: the defaults of the others go through it.
#[derive(Default)]
struct Bytes {
    h: u64,
}

impl Hasher for Bytes {
    fn finish(&self) -> u64 {
        self.h
    }
    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.h = (self.h ^ b as u64).wrapping_mul(0x100000001b3);
        }
    }
}

#[derive(Hash)]
enum One {
    A,
}

#[derive(Hash)]
enum Unit {
    A,
    B,
    C,
}

#[derive(Hash)]
enum Mixed {
    A(u8),
    B { x: u32, y: bool },
    C,
}

#[derive(Hash)]
#[repr(u8)]
enum Small {
    A = 3,
    B = 7,
}

#[derive(Hash)]
enum OneField {
    Only(u32),
}

#[derive(Hash)]
struct S {
    a: u8,
    b: (u32, char),
    c: [u8; 3],
    d: &'static str,
}

#[derive(Hash)]
struct Unitlike;

#[derive(Hash)]
struct Gen<T>(T, Option<T>);

fn show<T: Hash>(label: &str, t: &T) {
    unsafe { printf(c"%.*s\n".as_ptr(), label.len() as i32, label.as_ptr()) };
    let mut r = Rec::default();
    t.hash(&mut r);
    let mut b = Bytes::default();
    t.hash(&mut b);
    unsafe { printf(c"  = %llu %llu\n".as_ptr(), r.finish(), b.finish()) };
}

fn main() {
    show("u8", &5u8);
    show("i16", &-2i16);
    show("u64", &7u64);
    show("i128", &-1i128);
    show("usize", &9usize);
    show("bool", &true);
    show("char", &'é');
    show("str", &"hi");
    show("unit", &());
    show("tuple", &(1u8, 2u32, "x"));
    show("array", &[1u32, 2, 3]);
    show("slice u8", &&[4u8, 5][..]);
    show("slice str", &&["a", "bc"][..]);
    show("ref", &&&3u32);
    show("option", &Some(3u32));
    show("none", &None::<u32>);
    show("ok", &Ok::<u8, u32>(1));
    show("err", &Err::<u8, u32>(2));
    show("one", &One::A);
    show("unit B", &Unit::B);
    show("mixed A", &Mixed::A(4));
    show("mixed B", &Mixed::B { x: 5, y: false });
    show("mixed C", &Mixed::C);
    show("small", &Small::B);
    show("onefield", &OneField::Only(6));
    show("struct", &S { a: 1, b: (2, 'c'), c: [7, 8, 9], d: "zz" });
    show("unitlike", &Unitlike);
    show("gen", &Gen(1u8, Some(2u8)));
    show("ordering", &core::cmp::Ordering::Less);
    show("reverse", &core::cmp::Reverse(3u8));
    let build = BuildHasherDefault::<Bytes>::default();
    unsafe { printf(c"hash_one %llu\n".as_ptr(), build.hash_one("abc")) };
    let mut h = build.build_hasher();
    h.write_u16(0x1234);
    h.write_i64(-1);
    h.write_u128(5);
    unsafe { printf(c"writes %llu\n".as_ptr(), h.finish()) };
}
