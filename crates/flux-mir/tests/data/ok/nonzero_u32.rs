use std::num::NonZeroU32;

enum Payload {
    Empty,
    Value(NonZeroU32),
}

struct Wrapped(NonZeroU32);

const ZERO: Option<NonZeroU32> = NonZeroU32::new(0);
const MAX: Option<NonZeroU32> = NonZeroU32::new(u32::MAX);
const _: () = assert!(std::mem::size_of::<NonZeroU32>() == 4);
const _: () = assert!(std::mem::size_of::<Option<NonZeroU32>>() == 4);
const _: () = assert!(std::mem::size_of::<Option<Option<NonZeroU32>>>() == 8);
const _: () = assert!(std::mem::size_of::<Payload>() == 4);
const _: () = assert!(std::mem::size_of::<Option<Payload>>() == 8);
const _: () = assert!(std::mem::size_of::<Option<Wrapped>>() == 4);

fn check(value: u32) -> u32 {
    let nz = NonZeroU32::new(value);
    let nested = Some(nz);
    let payload = match nz {
        Some(n) => Payload::Value(n),
        None => Payload::Empty,
    };
    let result = match nested {
        None => 0,
        Some(None) => 0,
        Some(Some(n)) => n.get(),
    };
    match payload {
        Payload::Empty => result,
        Payload::Value(n) => Wrapped(n).0.get(),
    }
}

fn main() {
    assert!(ZERO.is_none());
    assert!(MAX.unwrap().get() == u32::MAX);
    for value in [0, 1, 0x8000_0000, u32::MAX] {
        assert!(check(value) == value);
    }
}
