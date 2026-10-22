//! MISSING FEATURES. `core::cell::Cell` (interior mutability, the usual
//! way for RAII guards to record into a shared log) and inline `const`
//! blocks as array repeat operands (`[const { Cell::new(0) }; 4]`).
//!
//! rustc: compiles; exits with status 3.
//! flux: "`core::cell` is not supported yet", "`const` blocks are not
//! supported yet", then a cascade "the method `get` on `_` is not
//! supported yet". Blocks round2/raii_state.rs.

use core::cell::Cell;

struct Guard<'a> {
    count: &'a Cell<i32>,
}

impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.count.set(self.count.get() + 1);
    }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let cells = [const { Cell::new(0) }; 4];
    {
        let _a = Guard { count: &cells[1] };
        let _b = Guard { count: &cells[1] };
        let _c = Guard { count: &cells[1] };
    }
    unsafe { exit(cells[1].get()) }
}
