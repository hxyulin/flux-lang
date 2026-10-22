use std::mem::{align_of, size_of};

#[repr(C)]
#[derive(Copy, Clone)]
struct Pair {
    first: u8,
    second: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Fallback {
    pair: Pair,
    byte1: u8,
    byte2: u8,
}

#[repr(C, align(16))]
#[derive(Copy, Clone)]
struct Vector([u8; 16]);

#[repr(C)]
#[derive(Copy, Clone)]
struct Packed<V> {
    pair: Pair,
    v1: V,
    v2: V,
    len: usize,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Neon(Packed<Vector>);

type Concrete = Neon;
#[repr(C)]
#[derive(Copy, Clone)]
union Kind {
    fallback: Fallback,
    neon: Concrete,
    bytes: [u8; 64],
    tiny: u8,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct Padded {
    byte: u8,
    word: u32,
}
#[derive(Copy, Clone)]
union Padding {
    padded: Padded,
    narrow: u8,
}

#[derive(Copy)]
struct CustomClone(u32);
impl Clone for CustomClone {
    fn clone(&self) -> Self {
        println!("field clone must not run");
        *self
    }
}
#[derive(Copy, Clone)]
union OpaqueClone {
    wide: CustomClone,
    byte: u8,
}

union Owned {
    value: Fallback,
    bytes: [u8; 4],
}
impl Drop for Owned {
    fn drop(&mut self) {
        println!("drop {}", unsafe { self.value.byte2 });
    }
}

fn bounce(k: Kind) -> Kind {
    k
}
fn mutate(k: &mut Kind, n: u8) {
    unsafe {
        k.neon.0.v1.0[3] = n;
    }
}
fn consume(k: Owned) {
    println!("owned {}", unsafe { k.value.byte1 });
}
fn main() {
    println!(
        "layout {} {} {} {}",
        size_of::<Kind>(),
        align_of::<Kind>(),
        size_of::<Neon>(),
        size_of::<Option<Kind>>()
    );
    println!(
        "array {} {}",
        size_of::<[Kind; 2]>(),
        align_of::<[Kind; 2]>()
    );
    let mut k = Kind {
        neon: Neon(Packed {
            pair: Pair {
                first: 2,
                second: 5,
            },
            v1: Vector([7; 16]),
            v2: Vector([9; 16]),
            len: 23,
        }),
    };
    unsafe {
        let base = &raw const k as usize;
        println!(
            "offset {} {} {}",
            &raw const k.neon as usize - base,
            &raw const k.fallback as usize - base,
            &raw const k.bytes as usize - base
        );
    }
    let original = bounce(k.clone());
    mutate(&mut k, 42);
    unsafe {
        let r = &mut k;
        let a = &mut r.neon.0.v1.0;
        let b = &mut r.neon.0.v2.0;
        a[0] = 11;
        b[1] = 13;
        println!(
            "vectors {} {} {} {}",
            k.neon.0.v1.0[0], k.neon.0.v1.0[3], k.neon.0.v2.0[1], original.neon.0.v1.0[3]
        );
        let read = || k.neon.0.v1.0[3];
        println!("borrow closure {}", read());
    }
    let moved = move || unsafe { original.neon.0.v2.0[0] + original.neon.0.pair.second };
    println!("move closure {} {}", moved(), moved());
    // Narrow initialization leaves the rest of storage uninitialized. Neither
    // Copy nor Clone may read inactive fields, padding, or invent initialized bytes.
    let small = Kind { tiny: 61 };
    let copy = bounce(small);
    let clone = small.clone();
    let captured = move || unsafe { copy.tiny };
    println!(
        "partial {} {} {}",
        unsafe { small.tiny },
        unsafe { clone.tiny },
        captured()
    );
    // Padding is never inspected, including after repeated opaque copies.
    let padded = Padding {
        padded: Padded {
            byte: 3,
            word: 123456,
        },
    };
    let padded_copy = padded.clone();
    println!(
        "padding {} {}",
        unsafe { padded_copy.padded.byte },
        unsafe { padded_copy.padded.word }
    );
    let opaque = OpaqueClone { byte: 27 };
    let opaque_copy = opaque.clone();
    println!("opaque clone {}", unsafe { opaque_copy.byte });
    let narrow = Padding { narrow: 19 };
    let narrow_copy = narrow.clone();
    println!("narrow {}", unsafe { narrow_copy.narrow });
    let mut bytes = Kind { bytes: [0; 64] };
    bytes.fallback = Fallback {
        pair: Pair {
            first: 4,
            second: 6,
        },
        byte1: 8,
        byte2: 10,
    };
    unsafe {
        println!(
            "overlap {} {} {} {} {}",
            bytes.bytes[0], bytes.bytes[1], bytes.bytes[2], bytes.bytes[3], bytes.bytes[63]
        );
    }
    let values = [bounce(bytes), bounce(bytes.clone())];
    unsafe {
        println!(
            "array values {} {}",
            values[0].fallback.pair.second, values[1].fallback.byte2
        );
    }
    consume(Owned {
        value: Fallback {
            pair: Pair {
                first: 0,
                second: 1,
            },
            byte1: 17,
            byte2: 21,
        },
    });
}
