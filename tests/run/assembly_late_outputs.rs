#[cfg(target_arch = "aarch64")]
fn main() {
    use std::arch::{asm, aarch64::*};
    let mut value: u64;
    // SAFETY: the assembly writes a general register and has no memory effects.
    unsafe { asm!("mov {0}, #{1}", lateout(reg) value, const 7, options(nomem, nostack)); }
    // SAFETY: the assembly changes only its input/output register.
    unsafe { asm!("add {0}, {0}, #3", inlateout(reg) value, options(nomem, nostack)); }
    let bytes = [4u8; 32];
    // SAFETY: bytes contains the thirty-two readable bytes required by this load.
    let loaded = unsafe { vld1q_u8_x2(bytes.as_ptr()) };
    let mut result: uint8x16_t;
    // SAFETY: cmeq reads two vectors and writes only the output vector.
    unsafe { asm!("cmeq {out:v}.16b, {a:v}.16b, {b:v}.16b", out=lateout(vreg) result,
        a=in(vreg) loaded.0, b=in(vreg) loaded.1, options(pure, nomem, nostack)); }
    let lanes: [u8; 16] = unsafe { std::mem::transmute(result) };
    println!("{} {}", value, lanes[0]);
}
#[cfg(not(target_arch = "aarch64"))]
fn main() {}
