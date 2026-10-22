//! Arrays and slices: literals, repeats, indexing (also as places),
//! `len` and `is_empty`, unsizing to slices, arrays in structs, enums,
//! tuples, parameters, return values, constants and generic code, and
//! the drops of arrays of values with destructors.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

const SIZE: usize = 4;
const DOUBLE: usize = SIZE * 2;
const PRIMES: [u32; 5] = [2, 3, 5, 7, 11];

struct Matrix {
    rows: [[i64; 3]; 3],
    scale: i64,
}

#[derive(Clone, Copy)]
struct Pixel {
    rgb: [u8; 3],
}

enum Shape {
    Empty,
    Points([i32; 4]),
}

struct Noisy {
    id: i32,
}

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe { printf(c"drop %d\n".as_ptr(), self.id) };
    }
}

trait Total {
    fn total(&self) -> i64;
}

impl Total for [u8] {
    fn total(&self) -> i64 {
        let mut sum = 0;
        let mut i = 0;
        while i < self.len() {
            sum += self[i] as i64;
            i += 1;
        }
        sum
    }
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

fn double_all(values: &mut [i32]) {
    let mut i = 0;
    while i < values.len() {
        values[i] *= 2;
        i += 1;
    }
}

fn reversed(values: [i32; 4]) -> [i32; 4] {
    [values[3], values[2], values[1], values[0]]
}

fn first<T: Copy>(values: &[T]) -> T {
    values[0]
}

fn pick<T>(values: [T; 3], which: usize) -> T
where
    T: Copy,
{
    values[which]
}

fn transpose(m: Matrix) -> Matrix {
    let mut rows = [[0; 3]; 3];
    let mut i = 0;
    while i < 3 {
        let mut j = 0;
        while j < 3 {
            rows[j][i] = m.rows[i][j] * m.scale;
            j += 1;
        }
        i += 1;
    }
    Matrix { rows, scale: 1 }
}

fn describe(shape: &Shape) -> i32 {
    match shape {
        Shape::Empty => -1,
        Shape::Points(points) => points[0] + points[3],
    }
}

fn maybe(flag: bool) -> Option<[u16; 2]> {
    if flag { Some([7, 9]) } else { None }
}

fn count_empty(values: &[u8]) -> bool {
    values.is_empty()
}

fn main() {
    let a = [1, 2, 3, 4];
    let total = sum(&a);
    unsafe { printf(c"sum %d len %d\n".as_ptr(), total, a.len() as i32) };

    let mut b = [5; SIZE];
    double_all(&mut b);
    b[0] += 1;
    let r = reversed(b);
    unsafe { printf(c"%d %d %d %d\n".as_ptr(), r[0], r[1], r[2], r[3]) };

    let big = [3u64; DOUBLE];
    let mut wide = [0u8; 100];
    wide[99] = 42;
    unsafe { printf(c"%d %d %d\n".as_ptr(), big[7] as i32, wide[99] as i32, wide.len() as i32) };

    let m = Matrix {
        rows: [[1, 2, 3], [4, 5, 6], [7, 8, 9]],
        scale: 10,
    };
    let t = transpose(m);
    unsafe { printf(c"%d %d %d\n".as_ptr(), t.rows[0][1] as i32, t.rows[2][0] as i32, t.scale as i32) };

    let p = Pixel { rgb: [10, 20, 30] };
    let mut q = p;
    q.rgb[1] = 99;
    unsafe { printf(c"%d %d\n".as_ptr(), p.rgb[1] as i32, q.rgb[1] as i32) };

    let shapes = [Shape::Points([1, 2, 3, 4]), Shape::Empty];
    unsafe { printf(c"%d %d\n".as_ptr(), describe(&shapes[0]), describe(&shapes[1])) };

    if let Some(pair) = maybe(true) {
        unsafe { printf(c"pair %d %d\n".as_ptr(), pair[0] as i32, pair[1] as i32) };
    }
    let none = maybe(false).is_none();
    unsafe { printf(c"none %d\n".as_ptr(), none as i32) };

    let words: [[u8; 2]; 2] = [[1, 2], [3, 4]];
    let flat: &[[u8; 2]] = &words;
    unsafe { printf(c"%d %d\n".as_ptr(), flat[1][0] as i32, flat.len() as i32) };

    let empty: [u8; 0] = [];
    unsafe { printf(c"%d %d\n".as_ptr(), count_empty(&empty) as i32, count_empty(&[1]) as i32) };

    unsafe { printf(c"%d %d\n".as_ptr(), first(&PRIMES) as i32, pick([PRIMES[1], PRIMES[2], PRIMES[4]], 2) as i32) };

    let tuple = (1u8, [2i16, 3]);
    let (x, pair) = tuple;
    let (y, z) = (pair[0], pair[1]);
    unsafe { printf(c"%d %d %d\n".as_ptr(), x as i32, y as i32, z as i32) };

    {
        let _noisy = [Noisy { id: 1 }, Noisy { id: 2 }, Noisy { id: 3 }];
        let mut more = [Noisy { id: 4 }, Noisy { id: 5 }];
        more[0] = Noisy { id: 6 };
        unsafe { printf(c"scope end\n".as_ptr()) };
    }
    let bytes: &[u8] = &[200, 100, 50];
    unsafe { printf(c"total %d\n".as_ptr(), bytes.total() as i32) };
    unsafe { printf(c"done\n".as_ptr()) };
}
