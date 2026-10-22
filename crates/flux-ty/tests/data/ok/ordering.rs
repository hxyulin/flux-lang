// `Ordering`, `PartialOrd` and `Ord` from `core`: `<` and friends on
// types other than the integers and `bool` call `PartialOrd`'s methods,
// references compare what they point to, `cmp`, `max`, `min` and `clamp`
// are `Ord`'s (so `i32::max` too), derived impls type-check with their
// fields', and `Ordering`'s methods are `const`.

use core::cmp::{self, Ordering};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
    major: u32,
    minor: u32,
}

#[derive(PartialEq, PartialOrd)]
struct Labeled<T> {
    label: u8,
    value: T,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Event {
    Start,
    Tick(u64),
    Stop { code: i32 },
}

#[derive(PartialEq, PartialOrd)]
enum Never {}

// A hand-written order, by `key` only.
struct Keyed {
    key: i32,
    _payload: bool,
}

impl PartialEq for Keyed {
    fn eq(&self, other: &Keyed) -> bool {
        self.key == other.key
    }
}

impl PartialOrd for Keyed {
    fn partial_cmp(&self, other: &Keyed) -> Option<Ordering> {
        self.key.partial_cmp(&other.key)
    }
}

const LESS: Ordering = Ordering::Greater.reverse();
const IS_LT: bool = LESS.is_lt() && Ordering::Equal.then(LESS).is_le();

const fn sign(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

fn integers(a: i32, b: i32, r: &u8, m: &mut u8) -> bool {
    let first: Ordering = a.cmp(&b);
    let partial: Option<Ordering> = a.partial_cmp(&b);
    let largest = a.max(b).max(i32::max(1, 2)).max(cmp::max(a, 3));
    let smallest = Ord::min(a, b).min(cmp::min(b, 4));
    let clamped = largest.clamp(smallest, 10);
    let literal = 5.max(a);
    first.is_lt() && partial.is_some() && clamped < literal && r < m && m >= &mut 3 && &a <= &b
}

fn generic<T: Ord>(a: T, b: T, c: &T) -> T {
    let ordered = a < b && a.le(&b) && c > c;
    if ordered { a.max(b) } else { b.min(a) }
}

fn partial<T: PartialOrd>(a: &T, b: &T) -> Option<Ordering> {
    if a < b && !(a >= b) { a.partial_cmp(b) } else { None }
}

fn derived(v: Version, e: Event) -> bool {
    let newer = Version { major: 2, minor: 0 };
    let label = Labeled { label: 1, value: v } < Labeled { label: 1, value: newer };
    let keyed = Keyed { key: 1, _payload: true } > Keyed { key: 0, _payload: false };
    let event: Ordering = e.cmp(&Event::Tick(3));
    label && keyed && event != Ordering::Equal && v.max(newer) == newer
}

fn never(n: &Never) -> bool {
    n <= n
}

fn options(a: Option<i32>, b: Result<u8, bool>) -> bool {
    a < Some(1) && a.max(None).is_none() && b >= Ok(0) && b.cmp(&Err(false)).is_lt()
}

fn main() {
    let mut m = 4;
    integers(1, 2, &3, &mut m);
    generic(1, 2, &3);
    partial(&1u8, &2);
    derived(Version { major: 1, minor: 2 }, Event::Start);
    options(Some(0), Ok(1));
    sign(LESS);
    let _ = IS_LT;
}
