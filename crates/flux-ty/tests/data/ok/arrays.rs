// Arrays and slices: types, literals, repeats, indexing, unsizing and
// the built-in `len` and `is_empty`.

const N: usize = 3;

struct Grid {
    cells: [[u8; 2]; N],
    count: usize,
}

#[derive(Clone, Copy)]
struct Point {
    coords: [i32; 2],
}

fn sum(values: &[i32]) -> i32 {
    let mut total = 0;
    let mut i = 0;
    while i < values.len() {
        total += values[i];
        i += 1;
    }
    total
}

fn first<T: Copy>(values: [T; 3]) -> T {
    values[0]
}

fn fill(values: &mut [u8], value: u8) {
    let mut i = 0;
    while i < values.len() {
        values[i] = value;
        i += 1;
    }
}

fn make() -> [i64; 4] {
    [1, 2, 3, 4]
}

fn main() {
    let a = [1, 2, 3];
    let b: [u8; N] = [0; N];
    let c = [true; 2];
    let empty: [i32; 0] = [];
    let index = 1;
    let x = a[index];
    let total = sum(&a);
    let mut buffer = [0u8; 8];
    fill(&mut buffer, 7);
    buffer[0] = b[1];
    let mut grid = Grid {
        cells: [[0; 2]; N],
        count: 0,
    };
    grid.cells[1][0] = 5;
    grid.count = grid.cells.len();
    let point = Point { coords: [3, 4] };
    let copy = point;
    let y = copy.coords[1] + point.coords[0];
    let f = first([c[0], c[1], empty.is_empty()]);
    let slice: &[i64] = &make();
    let r = &mut buffer;
    r[2] += 1;
    let n = r.len() + slice.len();
    let _ = (x, total, y, f, n, grid.count);
}
