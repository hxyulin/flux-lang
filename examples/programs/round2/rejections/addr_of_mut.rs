//! MISSING FEATURE, REPORTED AS A PLAIN ERROR. `core::ptr::addr_of_mut!`,
//! the edition-2024 way (with `&raw mut`, also unsupported) to get at a
//! large `static mut` buffer without a reference to it.
//!
//! rustc: compiles; exits with status 9.
//! flux: "cannot find macro `addr_of_mut` in this scope" (rustc's wording
//! for an unknown macro, on valid code). Blocks round2/lz77.rs.

static mut BUF: [u8; 1 << 16] = [0; 1 << 16];

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    // SAFETY: single-threaded, borrowed once.
    let buf = unsafe { &mut *core::ptr::addr_of_mut!(BUF) };
    buf[100] = 9;
    unsafe { exit(buf[100] as i32) }
}
