//! MISSING FEATURE. `core::mem::size_of` (here in an array length inside
//! an associated type, as a hashing macro writes it).
//!
//! rustc: compiles; exits with status 4.
//! flux: "`core::mem::size_of` is not supported yet" (reported twice at the
//! same position when a macro writes it, as round2/hashing.rs does).
//! Blocks round2/hashing.rs.

trait Digest {
    type Output;
    fn finish(&self) -> Self::Output;
}

struct Fnv32(u32);

impl Digest for Fnv32 {
    type Output = [u8; core::mem::size_of::<u32>()];
    fn finish(&self) -> Self::Output {
        self.0.to_be_bytes()
    }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    unsafe { exit(Fnv32(4).finish()[3] as i32) }
}
