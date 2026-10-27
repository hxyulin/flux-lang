use std::arch::aarch64::*;

#[inline(never)]
fn delimiters(bytes: &[u8]) -> u64 {
    // This is the NEON pipeline in pinned Keel's parser/scanning.rs.
    // SAFETY: NEON is baseline on AArch64 and the caller supplies sixteen bytes.
    unsafe {
        let input = vld1q_u8(bytes.as_ptr());
        let mut matches = vceqq_u8(input, vdupq_n_u8(b'\n'));
        for byte in *b"\r;/\"\\" {
            matches = vorrq_u8(matches, vceqq_u8(input, vdupq_n_u8(byte)));
        }
        vget_lane_u64::<0>(vreinterpret_u64_u8(vshrn_n_u16::<4>(vreinterpretq_u16_u8(
            matches,
        ))))
    }
}

fn scalar(bytes: &[u8]) -> u64 {
    let mut mask = 0;
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(*byte, b'\n' | b'\r' | b';' | b'/' | b'"' | b'\\') {
            mask |= 15u64 << (index * 4);
        }
    }
    mask
}

#[inline(never)]
fn through_pointers(bytes: &[u8]) -> u64 {
    let load: unsafe fn(*const u8) -> uint8x16_t = vld1q_u8;
    let splat: unsafe fn(u8) -> uint8x16_t = vdupq_n_u8;
    let equal: unsafe fn(uint8x16_t, uint8x16_t) -> uint8x16_t = vceqq_u8;
    let union: unsafe fn(uint8x16_t, uint8x16_t) -> uint8x16_t = vorrq_u8;
    let halfwords: unsafe fn(uint8x16_t) -> uint16x8_t = vreinterpretq_u16_u8;
    let narrow: unsafe fn(uint16x8_t) -> uint8x8_t = vshrn_n_u16::<4>;
    let bits: unsafe fn(uint8x8_t) -> uint64x1_t = vreinterpret_u64_u8;
    let lane: unsafe fn(uint64x1_t) -> u64 = vget_lane_u64::<0>;
    // SAFETY: all sixteen bytes are readable, and the constant shift and lane are valid.
    unsafe {
        let input = load(bytes.as_ptr());
        let mut matches = equal(input, splat(b'\n'));
        for byte in *b"\r;/\"\\" {
            matches = union(matches, equal(input, splat(byte)));
        }
        lane(bits(narrow(halfwords(matches))))
    }
}

fn main() {
    let mut bytes = [b'a'; 48];
    let mut checks = 0u32;
    for offset in 0..16 {
        for index in 0..16 {
            for byte in 0..=255u8 {
                bytes[offset + index] = byte;
                let block = &bytes[offset..offset + 16];
                let expected = scalar(block);
                assert_eq!(delimiters(block), expected);
                assert_eq!(through_pointers(block), expected);
                checks += 1;
            }
            bytes[offset + index] = b'a';
        }
    }
    bytes.fill(b';');
    assert_eq!(delimiters(&bytes[1..17]), u64::MAX);
    assert_eq!(through_pointers(&bytes[1..17]), u64::MAX);
    println!("scanner masks {}", checks);
}
