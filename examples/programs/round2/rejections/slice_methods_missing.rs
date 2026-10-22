//! MISSING FEATURES. Slice methods flux's `core` lacks, reported in two
//! different ways:
//! - `chunks_exact_mut`: "the method `chunks_exact_mut` on `[u8; 8]` is not
//!   supported yet", with cascades "the method `copy_from_slice` on `_`"
//!   and "`to_le_bytes` on `_` is not supported yet" in round2/hashing.rs;
//! - `copy_within`: "no method named `copy_within` found for array
//!   `[T; N]` in the current scope" (rustc's E0599 wording on valid code)
//!   on a generic array in round2/fixed_vec.rs, but "the method
//!   `copy_within` on `[{integer}; 4]` is not supported yet" here;
//! - `get` with a range: "mismatched types: expected `usize`, found
//!   `Range<usize>`" (a plain E0308 on valid code: flux's `get` takes only a
//!   `usize`, where rustc's takes any `SliceIndex`), followed by "cannot
//!   index into a value of type `&u8`" in the closure.
//!
//! rustc: compiles; exits with status 9.
//! Found in round2/hashing.rs, round2/fixed_vec.rs and round2/json.rs.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let mut block = [0u8; 8];
    for (i, chunk) in block.chunks_exact_mut(4).enumerate() {
        chunk.copy_from_slice(&(i as u32 + 1).to_le_bytes());
    }
    let mut a = [1, 2, 3, 4];
    a.copy_within(1..4, 0);
    let window = block.get(0..2).map_or(0, |w| w[0] as i32);
    unsafe { exit(window + a[2] + block[4] as i32 + 2) }
}
