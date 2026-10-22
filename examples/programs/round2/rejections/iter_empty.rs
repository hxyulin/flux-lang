//! MISSING FEATURE. `core::iter::empty` (an empty input stream for an
//! interpreter taking `&mut impl Iterator<Item = u8>`).
//!
//! rustc: compiles; exits with status 0.
//! flux: "`core::iter::empty` is not supported yet". Blocks
//! round2/brainfuck.rs (`None.into_iter()` works instead).

fn first<I: Iterator<Item = u8>>(input: &mut I) -> u8 {
    input.next().unwrap_or(0)
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    unsafe { exit(first(&mut core::iter::empty()) as i32) }
}
