// A range of an array borrows the array, and its base is borrowed before
// the index is evaluated, without a two-phase borrow, as in rustc: so
// `&mut s[..s.len() - 1]` is E0502 and `g[g.w] = 1` E0503.

use core::ops::{Index, IndexMut};

struct G {
    cells: [i32; 4],
    w: usize,
}

impl Index<usize> for G {
    type Output = i32;
    fn index(&self, i: usize) -> &i32 {
        &self.cells[i]
    }
}

impl IndexMut<usize> for G {
    fn index_mut(&mut self, i: usize) -> &mut i32 {
        &mut self.cells[i]
    }
}

fn main() {
    let mut a = [1, 2, 3, 4];
    let s = &a[1..];
    a[0] = 5;
    let _ = s.len();
    let m = &mut a[..2];
    let n = &a[2..];
    m[0] = 1;
    let _ = n.len();
    let v = [1, 2, 3];
    let w = &v[1..];
    w[0] = 3;
    let t: &mut [i32] = &mut a;
    let u = &mut t[..t.len() - 1];
    u[0] = 3;
    let mut g = G { cells: [0; 4], w: 2 };
    g[g.w] = 1;
}
