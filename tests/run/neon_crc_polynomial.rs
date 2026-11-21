#[cfg(target_arch = "aarch64")]
fn main() {
    use std::arch::aarch64::*;

    if std::arch::is_aarch64_feature_detected!("crc") {
        // SAFETY: the runtime check permits the CRC instructions.
        unsafe {
            let initial = 0x1234_5678;
            let halfword = __crc32h(initial, 0x89ab);
            assert_eq!(halfword, __crc32b(__crc32b(initial, 0xab), 0x89));
            let word = __crc32w(initial, 0x89ab_cdef);
            assert_eq!(word, __crc32h(__crc32h(initial, 0xcdef), 0x89ab));
            println!("{halfword:08x} {word:08x}");
        }
    }

    if std::arch::is_aarch64_feature_detected!("aes") {
        // SAFETY: detection covers PMULL and the reinterpreted lanes accept all bits.
        unsafe {
            let a = 0xc123_4567_89ab_cdef;
            let b = 0x8123_4567_89ab_cdef;
            let expected = (0..64).fold(0u128, |bits, i| {
                if (b >> i) & 1 != 0 { bits ^ ((a as u128) << i) } else { bits }
            });
            let low = vmull_p64(a, b);
            let left = vreinterpretq_p64_u64(vsetq_lane_u64::<1>(a, vdupq_n_u64(7)));
            let right = vreinterpretq_p64_u64(vsetq_lane_u64::<1>(b, vdupq_n_u64(9)));
            let high = vmull_high_p64(left, right);
            let lanes: [u64; 2] = std::mem::transmute(vreinterpretq_u64_p128(high));
            assert_eq!(low, expected);
            assert_eq!(high, expected);
            assert_eq!(lanes, [expected as u64, (expected >> 64) as u64]);
            println!("{low:032x} {high:032x}");
        }
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn main() {}
