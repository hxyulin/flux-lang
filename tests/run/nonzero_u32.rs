//! Nonzero validity, exhausted niches, aggregate layout and boundary values.
use std::num::NonZeroU32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Payload {
    Empty,
    Value(NonZeroU32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Wrapped {
    value: NonZeroU32,
}

const ZERO: Option<NonZeroU32> = NonZeroU32::new(0);
const MAX: Option<NonZeroU32> = NonZeroU32::new(u32::MAX);
const _: () = assert!(std::mem::size_of::<NonZeroU32>() == 4);
const _: () = assert!(std::mem::size_of::<Option<NonZeroU32>>() == 4);
const _: () = assert!(std::mem::size_of::<Option<Option<NonZeroU32>>>() == 8);
const _: () = assert!(std::mem::size_of::<Payload>() == 4);
const _: () = assert!(std::mem::size_of::<Option<Payload>>() == 8);
const _: () = assert!(std::mem::size_of::<Option<Wrapped>>() == 4);

fn check(value: u32) {
    let value = std::hint::black_box(value);
    let nz = NonZeroU32::new(value);
    println!("new {value}: {nz:?}");
    let payload = match nz {
        Some(n) => {
            assert_eq!(n.get(), value);
            let copied = n;
            assert_eq!(copied.clone(), n);
            println!("value {n} {n:08} {n:?}");
            assert!(n <= NonZeroU32::new(u32::MAX).unwrap());
            let wrapped = Some(Wrapped { value: n });
            assert_eq!(wrapped.unwrap().value.get(), value);
            Payload::Value(n)
        }
        None => {
            assert_eq!(value, 0);
            Payload::Empty
        }
    };
    println!("payload {payload:?}");
    let nested = [None, Some(None), Some(nz)];
    for entry in nested {
        println!("nested {entry:?}");
        match entry {
            None => (),
            Some(None) => (),
            Some(Some(n)) => assert_eq!(n.get(), value),
        }
    }
    for entry in [None, Some(Payload::Empty), Some(payload)] {
        println!("enum {entry:?}");
    }
}

fn main() {
    assert_eq!(ZERO, None);
    assert_eq!(MAX.unwrap().get(), u32::MAX);
    for value in [
        0,
        1,
        2,
        255,
        256,
        65535,
        65536,
        0x7fff_ffff,
        0x8000_0000,
        u32::MAX - 1,
        u32::MAX,
    ] {
        check(value);
    }
}
