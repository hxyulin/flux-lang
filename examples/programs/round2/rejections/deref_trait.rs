//! MISSING FEATURE. Implementing `Deref`/`DerefMut` (a container that
//! derefs to a slice, so slice methods work on it).
//!
//! rustc: compiles; exits with status 3.
//! flux: "`core::ops::Deref` is not supported yet" (and `DerefMut`).
//! Blocks round2/fixed_vec.rs.

use core::ops::Deref;

struct Wrapper {
    items: [i32; 4],
    len: usize,
}

impl Deref for Wrapper {
    type Target = [i32];
    fn deref(&self) -> &[i32] {
        &self.items[..self.len]
    }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let w = Wrapper { items: [1, 2, 3, 4], len: 3 };
    unsafe { exit(w.len() as i32) }
}
