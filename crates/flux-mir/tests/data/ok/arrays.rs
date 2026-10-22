// Arrays and slices in MIR: bounds checks, `len`, unsizing, repeats,
// drops of arrays, and lengths and values from constant evaluation.

struct Noisy;

impl Drop for Noisy {
    fn drop(&mut self) {}
}

const fn cells(rows: usize) -> usize {
    rows * 4
}

const CELLS: usize = cells(2);
const TABLE: [u8; 3] = [1, 2, 3];
const SECOND: u8 = TABLE[1];
const LONG: usize = [0u16; CELLS].len();

const fn sum(values: [i32; 3]) -> i32 {
    let mut total = 0;
    let mut i = 0;
    while i < 3 {
        total += values[i];
        i += 1;
    }
    total
}

const TOTAL: i32 = sum([1, 2, 3]);

fn get(values: &[u32], index: usize) -> u32 {
    values[index]
}

fn grid() -> [u8; CELLS] {
    [SECOND; CELLS]
}

fn main() {
    let mut a = [1u32, 2, 3];
    a[0] = get(&a, 2);
    let n = a.len() + LONG;
    let g = grid();
    let _ = (n, g[7], TOTAL);
    let _noisy = [Noisy, Noisy];
}
