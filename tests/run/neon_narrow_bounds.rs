use core::arch::aarch64::*;

#[repr(C)]
struct Guarded {
    before: u64,
    value: uint8x8_t,
    after: u64,
}

#[inline(never)]
fn store<const N: i32>(out: &mut uint8x8_t, input: uint16x8_t) {
    // SAFETY: NEON is baseline on AArch64; callers use shifts 1..=8.
    *out = unsafe { vshrn_n_u16::<N>(input) };
}

#[inline(never)]
fn bits(value: uint8x8_t) -> u64 {
    // SAFETY: reinterpretation preserves all eight bytes, and lane zero exists.
    unsafe { vget_lane_u64::<0>(vreinterpret_u64_u8(value)) }
}

#[inline(never)]
fn check<const N: i32>(input: [u16; 8]) {
    // SAFETY: the source has exactly sixteen initialized readable bytes.
    let vector = unsafe { vreinterpretq_u16_u8(vld1q_u8(input.as_ptr() as *const u8)) };
    // SAFETY: NEON is baseline on AArch64 and the shift is valid.
    let zero = unsafe { vshrn_n_u16::<1>(vreinterpretq_u16_u8(vdupq_n_u8(0))) };
    let mut guarded = Guarded {
        before: 0x0123456789abcdef,
        value: zero,
        after: 0xfedcba9876543210,
    };
    // A separately allocated result ends exactly eight bytes after its start.
    let mut exact = Box::new(zero);
    assert_eq!(core::mem::size_of_val(&*exact), 8);
    store::<N>(&mut guarded.value, vector);
    store::<N>(&mut exact, vector);
    let mut expected = 0u64;
    let mut i = 0;
    while i < 8 {
        expected |= (((input[i] >> N) as u8) as u64) << (i * 8);
        i += 1;
    }
    assert_eq!(guarded.before, 0x0123456789abcdef);
    assert_eq!(guarded.after, 0xfedcba9876543210);
    assert_eq!(bits(guarded.value), expected);
    assert_eq!(bits(*exact), expected);
    println!("{} {} {}", N, bits(guarded.value), bits(*exact));
}

fn main() {
    let input = [65535, 0x8000, 0x7fff, 0x0100, 0x00ff, 0xabcd, 0x1234, 1];
    check::<1>(input);
    check::<2>(input);
    check::<3>(input);
    check::<4>(input);
    check::<5>(input);
    check::<6>(input);
    check::<7>(input);
    check::<8>(input);
}
