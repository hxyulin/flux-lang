// Assigning to or mutably borrowing an index of a type that implements
// `Index` but not `IndexMut` (E0594, E0596), with rustc's messages.

use core::ops::Index;

struct G {
    cells: [i32; 4],
}

impl Index<usize> for G {
    type Output = i32;
    fn index(&self, i: usize) -> &i32 {
        &self.cells[i]
    }
}

fn main() {
    let mut g = G { cells: [0; 4] };
    g[0] = 1;
    let r = &mut g[1];
    g[2] += 1;
    let _ = r;
}
