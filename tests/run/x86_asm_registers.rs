#[cfg(target_arch = "x86_64")]
fn main() {
    use core::arch::{asm, x86_64::*};
    let mut byte = 0xa5u8;
    // SAFETY: the instruction only updates the declared byte register.
    unsafe { asm!("xor {0}, 0xff", inout(reg_byte) byte, options(pure, nomem, nostack)); }
    assert_eq!(byte, 0x5a);
    // SAFETY: the opaque register barrier preserves its declared input.
    unsafe { asm!("/* {0} */", inout(reg_byte) byte, options(pure, nomem, nostack, preserves_flags)); }
    assert_eq!(byte, 0x5a);
    let a = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    let b = [1u8, 0, 3, 0, 5, 0, 7, 0, 9, 0, 11, 0, 13, 0, 15, 0];
    // SAFETY: the pointers each reference sixteen initialized bytes.
    let (a, b) = unsafe { (_mm_loadu_si128(a.as_ptr().cast()), _mm_loadu_si128(b.as_ptr().cast())) };
    let equal: __m128i;
    // SAFETY: SSE2 is baseline on x86-64; only the declared registers are used.
    unsafe { asm!("pcmpeqb {a}, {b}", a = inlateout(xmm_reg) a => equal, b = in(xmm_reg) b, options(pure, nomem, nostack, preserves_flags)); }
    let mask: u32;
    // SAFETY: SSE2 only reads the vector input and writes the integer output.
    unsafe { asm!("pmovmskb {mask:e}, {a}", mask = lateout(reg) mask, a = in(xmm_reg) equal, options(pure, nomem, nostack, preserves_flags)); }
    assert_eq!(mask, 0x5555);
    let mut masked = equal;
    // SAFETY: SSE2 only changes the declared vector register.
    unsafe { asm!("pand {a:x}, {b:x}", a = inout(xmm_reg) masked, b = in(xmm_reg) b, options(pure, nomem, nostack, preserves_flags)); }
    // SAFETY: both types contain sixteen bytes and every byte array is valid.
    let lanes: [u8; 16] = unsafe { core::mem::transmute(masked) };
    assert_eq!(lanes, [1, 0, 3, 0, 5, 0, 7, 0, 9, 0, 11, 0, 13, 0, 15, 0]);
    let input = 3u64;
    let early: u64;
    // SAFETY: all accesses stay within the declared registers. The first
    // instruction writes the output before the unrelated input is read.
    unsafe { asm!("add {out:r}, 1", "add {out:r}, {input:r}", out = inout(reg) input => early, input = in(reg) input, options(pure, nomem, nostack)); }
    assert_eq!(early, 7);
    let mut outputs = [0u8; 2];
    let mut input_calls = 0;
    {
        let mut update = || {
            // SAFETY: only the declared byte register is changed.
            unsafe { asm!("add {0}, 1", inout(reg_byte) { input_calls += 1; 5u8 } => outputs[1], options(pure, nomem, nostack)); }
        };
        update();
    }
    assert_eq!(input_calls, 1);
    assert_eq!(outputs, [0, 6]);
    let mut original = 8u8;
    // SAFETY: the byte register is the only state touched by the assembly.
    unsafe { asm!("add {0}, 1", inout(reg_byte) original => outputs[{ original = 100; 0 }], options(pure, nomem, nostack)); }
    assert_eq!(original, 100);
    assert_eq!(outputs[0], 9);
    println!("byte {byte} mask {mask} lanes {lanes:?}");
}
#[cfg(not(target_arch = "x86_64"))]
fn main() {}
