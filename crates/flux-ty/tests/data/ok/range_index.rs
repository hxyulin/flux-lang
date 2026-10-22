// Ranges as indices of arrays and slices, through `core`'s `Index` and
// `IndexMut` impls for `[T]` (an array is unsized to a slice for them),
// and `Index`/`IndexMut` implemented by the program.

use core::ops::{Index, IndexMut};

struct Grid {
    cells: [u8; 6],
    width: usize,
}

impl Index<(usize, usize)> for Grid {
    type Output = u8;

    fn index(&self, (row, col): (usize, usize)) -> &u8 {
        &self.cells[row * self.width + col]
    }
}

impl IndexMut<(usize, usize)> for Grid {
    fn index_mut(&mut self, (row, col): (usize, usize)) -> &mut u8 {
        &mut self.cells[row * self.width + col]
    }
}

fn middle(values: &[i32]) -> &[i32] {
    &values[1..values.len() - 1]
}

fn generic<C: Index<usize, Output = i32>>(c: &C) -> i32 {
    c[0] + c[1]
}

fn main() {
    let mut a = [1, 2, 3, 4, 5];
    let b = &a[1..3];
    let c = &a[..2];
    let d = &a[3..];
    let e = &a[..];
    let f = &a[1..=2];
    let g = &a[..=3];
    let first = a[2..][0];
    let len = a[1..4].len();
    let m = &mut a[2..4];
    m[0] = 9;
    a[..2].iter_mut().for_each(|x| *x = 0);
    a[3..][1] = 7;
    let mid = middle(&a);
    let mut grid = Grid { cells: [0; 6], width: 3 };
    grid[(1, 2)] = 5;
    grid[(0, 0)] += 1;
    let corner = grid[(1, 2)];
    let r = &mut grid[(0, 1)];
    *r = 4;
}
