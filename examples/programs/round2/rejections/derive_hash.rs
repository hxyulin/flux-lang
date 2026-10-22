//! MISSING FEATURE (documented). `#[derive(Hash)]`, written out of habit
//! on a plain enum even where nothing hashes it.
//!
//! rustc: compiles; exits with status 1.
//! flux: "deriving `Hash` is not supported yet". Blocks
//! round2/inventory.rs.

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Category {
    Tools,
    Garden,
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let c = Category::Garden;
    unsafe { exit((c == Category::Garden) as i32 + (c == Category::Tools) as i32) }
}
