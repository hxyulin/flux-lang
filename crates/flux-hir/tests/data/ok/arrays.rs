// Array and slice types, array and repeat expressions, and indexing.

const N: usize = 2;

mod sizes {
    pub const WIDE: usize = 8;
}

struct Buffer {
    bytes: [u8; sizes::WIDE],
    rows: [[i32; N]; 3usize],
}

fn total(values: &[i32], counts: &mut [u8]) -> i32 {
    counts[0] += 1;
    values[0] + values[values.len() - 1]
}

fn main() {
    let a = [1, 2, 3];
    let b = [0u8; N];
    let mut buffer = Buffer {
        bytes: [0; sizes::WIDE],
        rows: [[1, 2], [3, 4], [5, 6]],
    };
    buffer.rows[1][0] = a[2];
    let mut counts = b;
    let _ = total(&a, &mut counts);
    let _ = (buffer.bytes[(N)], [(); 0]);
}
