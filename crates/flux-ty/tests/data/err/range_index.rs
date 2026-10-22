// Indexing errors: a slice indexed by a range of another type than
// `usize` (E0277, with rustc's path for the range), a type without
// `Index` (E0608), and an overloaded index in a constant, which rustc's
// `core` makes conditionally `const` (E0658).

struct H;

const A: [i32; 3] = [1, 2, 3];
const S: &[i32] = &A[1..];

fn main() {
    let a = [1, 2, 3];
    let b = &a[1i32..2];
    let h = H;
    let x = h[0];
    let s: &[u8] = &[4, 5];
    let t = &s[..2u8];
}
