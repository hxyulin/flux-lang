use core::arch::aarch64::*;

fn narrow<const N: i32>(a: uint16x8_t) -> uint8x8_t {
    // SAFETY: NEON is baseline on the tested AArch64 target.
    unsafe { vshrn_n_u16::<N>(a) }
}

fn narrow_fn<F: Fn(uint16x8_t) -> uint8x8_t>(f: F, a: uint16x8_t) -> uint8x8_t {
    f(a)
}

fn pair(a: uint8x16_t, b: uint8x16_t) -> uint8x16_t {
    // SAFETY: NEON is baseline on the tested AArch64 target.
    unsafe { vpmaxq_u8(a, b) }
}

fn pair_fn<F: Fn(uint8x16_t, uint8x16_t) -> uint8x16_t>(
    f: F,
    a: uint8x16_t,
    b: uint8x16_t,
) -> uint8x16_t {
    f(a, b)
}

#[inline(never)]
fn check_shift<const N: i32>(input: [u16; 8]) {
    // SAFETY: the array contains sixteen readable bytes; the load is unaligned.
    let a = unsafe { vreinterpretq_u16_u8(vld1q_u8(input.as_ptr() as *const u8)) };
    let f: unsafe fn(uint16x8_t) -> uint8x8_t = vshrn_n_u16::<N>;
    // SAFETY: NEON is baseline on the tested AArch64 target.
    let outputs = unsafe {
        [
            vshrn_n_u16::<N>(a),
            narrow::<N>(a),
            f(a),
            narrow_fn(narrow::<N>, a),
        ]
    };
    let mut j = 0;
    while j < outputs.len() {
        let borrowed = &outputs[j];
        let copy = *borrowed;
        // SAFETY: an eight-byte vector has exactly eight initialized bytes.
        let lanes = unsafe { *(&copy as *const uint8x8_t as *const [u8; 8]) };
        let mut i = 0;
        while i < 8 {
            assert_eq!(lanes[i], (input[i] >> N) as u8);
            i += 1;
        }
        println!("{} {:?}", N, lanes);
        j += 1;
    }
}

#[inline(never)]
fn check_pairs(a: [u8; 16], b: [u8; 16]) {
    // SAFETY: each source array contains sixteen readable bytes.
    let (va, vb) = unsafe { (vld1q_u8(a.as_ptr()), vld1q_u8(b.as_ptr())) };
    let f: unsafe fn(uint8x16_t, uint8x16_t) -> uint8x16_t = vpmaxq_u8;
    // SAFETY: NEON is baseline on the tested AArch64 target.
    let outputs = unsafe { [vpmaxq_u8(va, vb), f(va, vb), pair_fn(pair, va, vb)] };
    let mut j = 0;
    while j < outputs.len() {
        let borrowed = &outputs[j];
        let copy = *borrowed;
        // SAFETY: a sixteen-byte vector contains sixteen initialized bytes.
        let lanes = unsafe { *(&copy as *const uint8x16_t as *const [u8; 16]) };
        let mut i = 0;
        while i < 8 {
            let x = if a[2 * i] > a[2 * i + 1] {
                a[2 * i]
            } else {
                a[2 * i + 1]
            };
            let y = if b[2 * i] > b[2 * i + 1] {
                b[2 * i]
            } else {
                b[2 * i + 1]
            };
            assert_eq!(lanes[i], x);
            assert_eq!(lanes[i + 8], y);
            i += 1;
        }
        println!("{:?}", lanes);
        j += 1;
    }
}

fn main() {
    assert_eq!(core::mem::size_of::<uint8x8_t>(), 8);
    assert_eq!(core::mem::align_of::<uint8x8_t>(), 8);
    assert_eq!(core::mem::size_of::<uint16x8_t>(), 16);
    assert_eq!(core::mem::align_of::<uint16x8_t>(), 16);
    assert_eq!(core::mem::size_of::<uint8x16_t>(), 16);
    assert_eq!(core::mem::align_of::<uint8x16_t>(), 16);
    let input = [0, 65535, 32768, 255, 256, 0x1234, 0xabcd, 0x8001];
    check_shift::<1>(input);
    check_shift::<2>(input);
    check_shift::<3>(input);
    check_shift::<4>(input);
    check_shift::<5>(input);
    check_shift::<6>(input);
    check_shift::<7>(input);
    check_shift::<8>(input);
    check_pairs(
        [
            0, 255, 128, 127, 254, 129, 3, 2, 4, 200, 201, 5, 6, 202, 253, 7,
        ],
        [
            128, 0, 255, 1, 2, 254, 253, 3, 252, 4, 5, 251, 250, 6, 7, 249,
        ],
    );
    check_pairs([255; 16], [128; 16]);
    check_pairs(
        [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16],
        [
            32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17,
        ],
    );
}
