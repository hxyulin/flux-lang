//! MISSING FEATURE. `?Sized` bounds (a `Hash`-like trait implemented for
//! `str` and taken by reference).
//!
//! rustc: compiles; exits with status 5.
//! flux: "`?Sized` bounds are not supported yet", then "the size for values
//! of type `str` cannot be known at compilation time" at the call.
//! Blocks round2/hashmap.rs.

trait Weight {
    fn weight(&self) -> usize;
}

impl Weight for str {
    fn weight(&self) -> usize {
        self.len()
    }
}

fn weigh<T: Weight + ?Sized>(t: &T) -> usize {
    t.weight()
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    unsafe { exit(weigh("hello") as i32) }
}
