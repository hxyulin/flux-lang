//! Ranges as indices (`&a[1..3]`, `&mut a[lo..hi]`, `..=`, `..`) of arrays
//! and slices, through `core`'s `Index` and `IndexMut` for `[T]`, also
//! nested, in closures and as method receivers; and `Index`/`IndexMut`
//! implemented by the program.

use core::ops::{Index, IndexMut};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn sum(v: &[i64]) -> i64 {
    let mut t = 0;
    for x in v {
        t += *x;
    }
    t
}

fn fill(v: &mut [i64], value: i64) {
    for x in v.iter_mut() {
        *x = value;
    }
}

struct Grid {
    cells: [i32; 6],
    width: usize,
}

impl Index<(usize, usize)> for Grid {
    type Output = i32;
    fn index(&self, (r, c): (usize, usize)) -> &i32 {
        &self.cells[r * self.width + c]
    }
}

impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (r, c): (usize, usize)) -> &mut i32 {
        &mut self.cells[r * self.width + c]
    }
}

struct Wrap(Grid);

fn mid(v: &[i64], lo: usize, hi: usize) -> i64 {
    sum(&v[lo..hi]) + sum(&v[..lo]) * 100 + sum(&v[hi..]) * 10000 + sum(&v[..]) * 1000000
}

fn main() {
    let mut a = [1i64, 2, 3, 4, 5, 6, 7, 8];
    let m = mid(&a, 2, 5);
    fill(&mut a[1..3], 9);
    let inc = sum(&a[2..=4]) + sum(&a[..=1]);
    a[5..].iter_mut().for_each(|x| *x *= 2);
    let n = a[3..6].len() as i64;
    let first = a[4..][0];
    a[6..][1] = 100;
    let s: &[i64] = &a;
    let tail = s[s.len() - 2..].iter().fold(0, |t, x| t + *x);
    let mut g = Grid { cells: [0; 6], width: 3 };
    g[(1, 2)] = 7;
    g[(0, 1)] += 3;
    let w = Wrap(Grid { cells: [1, 2, 3, 4, 5, 6], width: 2 });
    let x = w.0[(2, 1)];
    let r = &mut g;
    r[(1, 0)] = 4;
    let gsum: i32 = g.cells.iter().fold(0, |t, x| t + *x);
    unsafe {
        printf(c"%lld %lld %lld %lld %lld %d %d %d\n".as_ptr(), m, inc, n, first, tail, g[(1, 2)], x, gsum);
    }
    for v in &a {
        unsafe { printf(c"%lld ".as_ptr(), *v) };
    }
    unsafe { printf(c"\n".as_ptr()) };
    closures();
}

fn closures() {
    let mut a = [1, 2, 3, 4, 5];
    let n = a[..a.len() - 1].len();
    let k = a.len() - 2;
    let m = &mut a[..k];
    m[0] = 10;
    let s: &[i32] = &a;
    let f = || s[1..].len() + s[..2][1] as usize;
    let mut b = [1, 2, 3];
    let mut g = || {
        b[1..].iter_mut().for_each(|x| *x += 1);
        b[0] = 7;
    };
    g();
    let h = |lo: usize| a[lo..].iter().fold(0, |t, x| t + *x);
    unsafe { printf(c"%zu %zu %d %d %d %d\n".as_ptr(), n, f(), b[0], b[2], h(1), a[0]) };
}
