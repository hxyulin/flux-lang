use core::mem::{align_of, size_of};
use core::ptr::addr_of;
use core::sync::atomic::{AtomicUsize, Ordering};
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct P {
    a: u8,
    b: u64,
    c: u16,
}
#[repr(C, packed(2))]
#[derive(Clone, Copy)]
struct P2 {
    a: u8,
    b: u64,
    c: u16,
}
#[repr(packed)]
#[derive(Clone, Copy)]
struct RustPack {
    a: u8,
    b: u64,
    c: u16,
}
#[repr(C, packed)]
struct Outer {
    a: u8,
    p: P2,
    tail: [u64; 2],
}
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Leaf(u64);
impl Drop for Leaf {
    fn drop(&mut self) {
        assert_eq!((self as *mut Self as usize) % align_of::<Self>(), 0);
        DROPS.fetch_add(self.0 as usize, Ordering::Relaxed);
    }
}
struct Mutating {
    a: Leaf,
    b: Leaf,
}
impl Drop for Mutating {
    fn drop(&mut self) {
        assert_eq!((self as *mut Self as usize) % align_of::<Self>(), 0);
        self.a.0 += 10;
    }
}
#[repr(C, packed)]
struct Owned {
    tag: u8,
    value: Mutating,
    tail: [Leaf; 2],
}
#[repr(packed)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Derived {
    a: u8,
    b: u64,
}
#[repr(packed)]
#[derive(Clone, Copy)]
struct RefPack<'a> {
    tag: u8,
    value: &'a u32,
}
#[repr(C, packed(2))]
#[derive(Clone, Copy)]
union PackedUnion {
    a: u64,
    b: [u8; 8],
}
#[repr(C, packed)]
struct PackedSlice {
    tag: u8,
    tail: [u64],
}
#[repr(C, packed)]
struct PackedStr {
    tag: u8,
    tail: str,
}
fn roundtrip(x: P) -> P {
    x
}
fn main() {
    assert_eq!((size_of::<P>(), align_of::<P>()), (11, 1));
    assert_eq!((size_of::<P2>(), align_of::<P2>()), (12, 2));
    let bytes = [1u8, 2, 0, 0, 0, 0, 0, 0, 0, 3, 0, 0, 0, 0, 0, 0, 0];
    let ptr = core::ptr::slice_from_raw_parts(bytes.as_ptr(), 2) as *const PackedSlice;
    let view = unsafe { &*ptr };
    assert_eq!(view.tail[0] + view.tail[1], 5);
    assert_eq!(core::mem::size_of_val(view), 17);
    let bytes = [7u8, b'a', b'b', b'c'];
    let ptr = core::ptr::slice_from_raw_parts(bytes.as_ptr(), 3) as *const PackedStr;
    let view = unsafe { &*ptr };
    assert_eq!(&view.tail, "abc");
    assert_eq!(core::mem::size_of_val(view), 4);
    let derived = Derived { a: 1, b: 2 };
    assert_eq!(derived, derived.clone());
    println!("{derived:?}");
    let number = 37;
    let packed = RefPack {
        tag: 1,
        value: &number,
    };
    let optional = Some(packed);
    let copied = optional;
    assert_eq!(*{ copied.unwrap().value }, 37);
    println!(
        "optional {} {}",
        size_of::<Option<RefPack<'_>>>(),
        align_of::<Option<RefPack<'_>>>()
    );
    let union = PackedUnion {
        a: 0x1122334455667788,
    };
    assert_eq!(unsafe { union.b }, 0x1122334455667788u64.to_ne_bytes());
    let mut p = P {
        a: 3,
        b: 0x1122334455667788,
        c: 9,
    };
    let base = addr_of!(p) as usize;
    println!(
        "{} {} {}",
        addr_of!(p.a) as usize - base,
        addr_of!(p.b) as usize - base,
        addr_of!(p.c) as usize - base
    );
    let got = p.b;
    assert_eq!(got, 0x1122334455667788);
    p.b = got + 1;
    let raw = &raw mut p.b;
    p.b += 1;
    unsafe {
        raw.write_unaligned(raw.read_unaligned() + 1);
    }
    assert_eq!({ p.b }, 0x112233445566778b);
    let p = roundtrip(p);
    assert_eq!({ p.c }, 9);
    let q = RustPack { a: 1, b: 2, c: 3 };
    let base = addr_of!(q) as usize;
    println!(
        "rust {} {} {} {} {}",
        size_of::<RustPack>(),
        align_of::<RustPack>(),
        addr_of!(q.a) as usize - base,
        addr_of!(q.b) as usize - base,
        addr_of!(q.c) as usize - base
    );
    let mut outer = Outer {
        a: 1,
        p: P2 { a: 2, b: 3, c: 4 },
        tail: [5, 6],
    };
    assert_eq!({ outer.p.b }, 3);
    outer.p.b = 7;
    outer.tail[1] = 9;
    assert_eq!({ outer.p.b }, 7);
    assert_eq!({ outer.tail[1] }, 9);
    let copy = outer.tail;
    assert_eq!(copy, [5, 9]);
    {
        let _owned = Owned {
            tag: 1,
            value: Mutating {
                a: Leaf(1),
                b: Leaf(2),
            },
            tail: [Leaf(3), Leaf(4)],
        };
    }
    assert_eq!(DROPS.load(Ordering::Relaxed), 20);
    println!(
        "packed {} {} {}",
        size_of::<Outer>(),
        align_of::<Outer>(),
        DROPS.load(Ordering::Relaxed)
    );
}
