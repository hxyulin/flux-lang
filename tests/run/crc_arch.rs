use std::arch::aarch64::{__crc32b, __crc32d};
use std::arch::is_aarch64_feature_detected as detected;

fn byte_reference(crc: u32, byte: u8) -> u32 {
    let mut crc = crc ^ byte as u32;
    for _ in 0..8 {
        crc = if crc & 1 != 0 {
            (crc >> 1) ^ 0xedb8_8320
        } else {
            crc >> 1
        };
    }
    crc
}

#[target_feature(enable = "crc")]
unsafe fn calculate(mut crc: u32, data: &[u8]) -> u32 {
    for byte in data {
        // SAFETY: this function requires the same CRC feature as the intrinsic.
        crc = unsafe { __crc32b(crc, *byte) };
    }
    crc
}

fn main() {
    let supported = std::arch::is_aarch64_feature_detected!("crc");
    assert_eq!(supported, detected!("crc"));
    if !supported {
        println!("crc unavailable");
        return;
    }
    let byte: unsafe fn(u32, u8) -> u32 = __crc32b;
    let word: unsafe fn(u32, u64) -> u32 = __crc32d;
    for seed in [0, 1, 0x1234_5678, 0x8000_0000, u32::MAX] {
        for value in 0..=255u8 {
            let expected = byte_reference(seed, value);
            // SAFETY: runtime detection above establishes the CRC feature.
            unsafe {
                assert_eq!(__crc32b(seed, value), expected);
                assert_eq!(byte(seed, value), expected);
            }
        }
    }
    let mut random = 0x0123_4567_89ab_cdefu64;
    let mut checksum = 0u32;
    for _ in 0..2048 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let seed = (random >> 32) as u32;
        let mut expected = seed;
        for byte in random.to_le_bytes() {
            expected = byte_reference(expected, byte);
        }
        // SAFETY: runtime detection above establishes the CRC feature.
        unsafe {
            assert_eq!(__crc32d(seed, random), expected);
            assert_eq!(word(seed, random), expected);
        }
        checksum ^= expected;
    }
    // SAFETY: runtime detection above establishes the CRC feature.
    assert_eq!(!unsafe { calculate(u32::MAX, b"123456789") }, 0xcbf4_3926);
    println!("crc {} {}", supported, checksum);
}
