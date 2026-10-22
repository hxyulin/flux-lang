// Indices known to be out of bounds: rustc's `unconditional_panic` lint
// in functions, and an error in constants.

fn constant_index(a: [i32; 3], s: &[i32]) -> i32 {
    let y = a[5];
    let i = 7;
    let z = a[i];
    // A slice's length is not known.
    let w = s[9];
    // Nor is a mutable local's, past its block.
    let mut j = 0;
    while j < 2 {
        j += 1;
    }
    y + z + w + a[j]
}

const C: i32 = [1, 2, 3][4];

const fn element(values: [i32; 2], i: usize) -> i32 {
    values[i]
}

const D: i32 = element([1, 2], 2);

fn main() {}
