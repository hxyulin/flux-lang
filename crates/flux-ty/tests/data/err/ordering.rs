// Comparisons with `<`, `<=`, `>` and `>=` that no `PartialOrd` impl
// takes, `#[derive(PartialOrd, Ord)]` on types whose fields or
// supertraits do not allow it, `Ord`'s methods where it does not hold,
// and comparisons in constants and `const fn`s, with rustc's errors.

use core::cmp::Ordering;

#[derive(PartialEq)]
struct OnlyEq(i32);

#[derive(PartialEq, Eq, PartialOrd)]
struct NotOrd(i32);

// E0277: `PartialOrd` needs `PartialEq`.
#[derive(PartialOrd)]
struct NoEq(i32);

// E0277: a field without `PartialOrd`.
#[derive(PartialEq, PartialOrd)]
struct Field {
    a: i32,
    b: OnlyEq,
}

// E0277: a field without `Ord`.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct OrdField(NotOrd);

// E0277: `Ord` needs `Eq` and `PartialOrd`.
#[derive(PartialEq, Ord)]
struct Bare(i32);

#[derive(PartialEq, PartialOrd)]
enum Variants {
    A(OnlyEq),
    B { x: i32 },
}

// E0369: no `PartialOrd` for the left operand at all.
fn no_impl(a: OnlyEq, b: OnlyEq, r: &OnlyEq) -> bool {
    a < b || r >= r
}

// E0308: the right operand must be what the left one's impl says, a
// reference of the same kind for a reference.
fn mismatched(r: &i32, n: NotOrd, x: i32, m: &mut i32) -> bool {
    r < 1 || r < &1u8 || n > 1 || x <= r || 1 < r || m > r
}

// E0369: a parameter has only its bounds' operators.
fn bounded<T: PartialEq>(a: T, b: T) -> bool {
    a < b
}

// E0599: no `clamp` without `Ord`.
fn no_ord(a: NotOrd, b: NotOrd, c: NotOrd) -> Ordering {
    let _m = a.clamp(b, c);
    Ordering::Less
}

// E0308: `max` takes its own type.
fn max_mismatch(x: i32, y: i64) -> i32 {
    x.max(y)
}

// Not supported yet, though rustc accepts them: comparing tuples and
// pointers, by reference too.
fn unsupported(t: &(i32, u8), p: *const u8) -> bool {
    t < t || t == t || p < p
}

// E0658 for `core`'s impls, which rustc's `core` makes conditionally
// `const`, E0015 for the program's.
const fn in_const_fn(a: i32, o: Ordering, n: NotOrd) -> bool {
    let _m = a.max(1);
    let _c = Ord::cmp(&a, &2);
    let _p = o < Ordering::Equal;
    let _d = n.partial_cmp(&NotOrd(1));
    o.is_lt() && o.reverse().is_gt()
}

const MAX: i32 = core::cmp::max(1, 2);

fn main() {}
