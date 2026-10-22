// Operators on references that no `impl` block of `core`'s takes, with
// rustc's errors: E0277 where the left operand's type has some impl of
// the trait, E0369, E0600 and E0368 where it has none, and E0308 where
// the referents are integers of different types, as for the values; and
// operators on references in constants and `const fn`s (E0658, E0015).

use core::ops::Add;

struct S(i32);

#[derive(PartialEq)]
struct V(i32);

impl Add for V {
    type Output = V;

    fn add(self, rhs: V) -> V {
        V(self.0 + rhs.0)
    }
}

// E0277 and E0308: integers of different types.
fn mixed(r: &i32, u: &u8, mut x: i32) {
    let _a = r + 1u8;
    let _b = 1u8 + r;
    let _c = r + u;
    x += u;
    let _d = r == &1u8;
}

// E0277: one side is not an integer.
fn not_integers(r: &i32) {
    let _a = r + true;
    let _b = r << true;
    let _c = 1 | &true;
}

// E0369 and E0600: no impl for the left operand's type at all.
fn no_impl(b: &bool, u: &u8, m: &mut i32, r: &i32, s: &S, v: &V) {
    let _a = b + 1;
    let _b = -u;
    let _c = -b;
    let _d = !s;
    let _e = m + 1;
    let _f = r + m;
    let _g = s + s;
    let _h = v + v;
}

// E0368: compound assignments to references.
fn compound_on_reference(r: &i32) {
    r += 1;
}

fn compound_on_mut_reference(m: &mut i32) {
    m -= 1;
}

// Comparisons: E0308 where the other side must be a reference too, E0277
// for `==`, which may compare with other types, and E0369.
fn comparisons(r: &i32, x: i32, s: &S) {
    let _a = r < 1;
    let _b = r < &1u8;
    let _c = 1 < r;
    let _d = x < r;
    let _e = r <= 5u8;
    let _f = s < s;
    let _g = r == 1;
}

// E0308: the result is the referent's type.
fn result_type(r: &i32) -> i64 {
    r + 1
}

// E0658 for `core`'s impls in constants, E0015 for the program's.
const C: i32 = 1 + &2;

const fn in_const_fn(r: &i32, s: &i32, mut x: i32) -> bool {
    x += r;
    let _a = -r;
    let _b = r << 1;
    r == s && r < s && x > 0
}

const fn user_impls(a: &V, b: &V) -> bool {
    a == b
}

fn main() {}
