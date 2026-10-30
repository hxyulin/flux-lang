use core::arch::{
    aarch64::{int32x4_t, uint8x8_t},
    asm,
};

fn main() {
    let lanes = [3i32, -5, 0x12345678, i32::MIN];
    // SAFETY: all four initialized i32 lanes exactly fill the vector.
    let mut vector: int32x4_t = unsafe { core::mem::transmute(lanes) };
    // SAFETY: the barrier reads and writes only its tied SIMD register.
    unsafe {
        asm!("/*{:v}*/", inout(vreg) vector, options(nomem, nostack));
    }
    // SAFETY: every vector lane is initialized and valid as an i32.
    let preserved: [i32; 4] = unsafe { core::mem::transmute(vector) };
    assert_eq!(preserved, lanes);

    // SAFETY: integer vector addition changes only the tied SIMD register.
    unsafe {
        asm!("add {0}.4s, {0}.4s, {0}.4s", inout(vreg) vector,
             options(nomem, nostack, preserves_flags));
    }
    // SAFETY: wrapping vector addition leaves four initialized i32 lanes.
    let doubled: [i32; 4] = unsafe { core::mem::transmute(vector) };
    for i in 0..4 {
        assert_eq!(doubled[i], lanes[i].wrapping_mul(2));
    }

    // SAFETY: REV64 rearranges bytes only inside its tied SIMD register.
    unsafe {
        asm!("rev64 {0:v}.4s, {0:v}.4s", inout(vreg) vector,
             options(nomem, nostack, preserves_flags));
    }
    // SAFETY: byte permutation preserves valid initialized i32 lanes.
    let shuffled: [i32; 4] = unsafe { core::mem::transmute(vector) };
    assert_eq!(shuffled, [doubled[1], doubled[0], doubled[3], doubled[2]]);

    let bytes = [1u8, 2, 0x7f, 0x80, 0xff, 19, 23, 29];
    // SAFETY: all eight initialized bytes exactly fill the D-vector.
    let mut short: uint8x8_t = unsafe { core::mem::transmute(bytes) };
    // SAFETY: the instruction rearranges bytes inside its tied D-vector only.
    unsafe {
        asm!("rev64 {0:v}.8b, {0:v}.8b", inout(vreg) short,
             options(nomem, nostack, preserves_flags));
    }
    // SAFETY: all bytes remain initialized after the permutation.
    let reversed: [u8; 8] = unsafe { core::mem::transmute(short) };
    assert_eq!(reversed, [29, 23, 19, 0xff, 0x80, 0x7f, 2, 1]);

    let mut scalar = 7u64;
    // SAFETY: the instructions update only the three tied registers. The two
    // different vector widths and scalar share one memory-resident MIR tuple.
    unsafe {
        asm!("add {0:v}.4s, {0:v}.4s, {0:v}.4s",
             "rev64 {1:v}.8b, {1:v}.8b",
             "add {2:x}, {2:x}, #9",
             inout(vreg) vector, inout(vreg) short, inout(reg) scalar,
             options(nomem, nostack, preserves_flags));
    }
    // SAFETY: both instructions retain initialized, valid integer lanes.
    let wide_result: [i32; 4] = unsafe { core::mem::transmute(vector) };
    // SAFETY: the D-vector's eight bytes remain initialized.
    let short_result: [u8; 8] = unsafe { core::mem::transmute(short) };
    for i in 0..4 {
        assert_eq!(wide_result[i], shuffled[i].wrapping_mul(2));
    }
    assert_eq!(short_result, bytes);
    assert_eq!(scalar, 16);

    let mut byte = 0xffu8;
    // SAFETY: this writes only its tied scalar and D-vector. The byte precedes
    // the vector in the result tuple, exercising physical field offsets.
    unsafe {
        asm!("add {0:w}, {0:w}, #3", "rev64 {1:v}.8b, {1:v}.8b",
             inout(reg) byte, inout(vreg) short,
             options(nomem, nostack, preserves_flags));
    }
    assert_eq!(byte, 2);
    // SAFETY: REV64 keeps all eight bytes initialized.
    let single_result: [u8; 8] = unsafe { core::mem::transmute(short) };
    assert_eq!(single_result, reversed);

    let mut float = -1.25f32;
    // SAFETY: scalar-only assembly retains its flattened MIR result path.
    unsafe {
        asm!("add {0:x}, {0:x}, #2", "/*{1:s}*/",
             inout(reg) scalar, inout(vreg) float,
             options(nomem, nostack, preserves_flags));
    }
    assert_eq!(scalar, 18);
    assert_eq!(float.to_bits(), (-1.25f32).to_bits());

    // SAFETY: these instructions modify only their tied registers. Scalar
    // transports must also be converted inside a memory-resident result tuple.
    unsafe {
        asm!("eor {0:w}, {0:w}, #0x80000000", "/*{1:b}*/",
             "rev64 {2:v}.8b, {2:v}.8b",
             inout(reg) float, inout(vreg) byte, inout(vreg) short,
             options(nomem, nostack, preserves_flags));
    }
    assert_eq!(float.to_bits(), 1.25f32.to_bits());
    assert_eq!(byte, 2);
    // SAFETY: all eight bytes remain initialized after the second reversal.
    let final_short: [u8; 8] = unsafe { core::mem::transmute(short) };
    assert_eq!(final_short, bytes);
    // SAFETY: the assembly copies all initialized vector lanes from an input
    // register to a tied output, without accessing memory.
    let mut copied=short;
    unsafe {
        asm!("orr {0:v}.8b, {1:v}.8b, {1:v}.8b", inout(vreg)copied, in(vreg)short,
            options(nomem,nostack,preserves_flags));
    }
    let copied_bytes:[u8;8]=unsafe { core::mem::transmute(copied) };
    assert_eq!(copied_bytes,bytes);
    println!(
        "vector asm {:?} {:?} {:?} {:?} {:?} {:?} {} {}",
        preserved, doubled, shuffled, reversed, wide_result, single_result, scalar, byte
    );
}
