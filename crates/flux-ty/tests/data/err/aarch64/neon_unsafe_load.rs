use core::arch::aarch64::*;
fn main() {
    let bytes = [0u8; 16];
    let _ = vld1q_u8(bytes.as_ptr());
}
