use core::arch::aarch64::*;
use core::mem::{align_of, size_of};

#[inline(never)]
unsafe fn combine(a: uint8x16_t, needle: u8) -> uint8x16_t {
    unsafe {
        let eq = vceqq_u8(a, vdupq_n_u8(needle));
        vorrq_u8(vandq_u8(a, vdupq_n_u8(0x55)), eq)
    }
}

#[inline(never)]
unsafe fn observe(v: uint8x16_t) -> (u64, u64) {
    unsafe {
        let words = vreinterpretq_u64_u8(v);
        (vgetq_lane_u64::<0>(words), vgetq_lane_u64::<1>(words))
    }
}

fn main() {
    println!(
        "{} {} {} {} {} {} {} {} {} {}",
        size_of::<uint8x16_t>(),
        align_of::<uint8x16_t>(),
        size_of::<uint16x8_t>(),
        align_of::<uint16x8_t>(),
        size_of::<uint64x2_t>(),
        align_of::<uint64x2_t>(),
        size_of::<uint8x8_t>(),
        align_of::<uint8x8_t>(),
        size_of::<uint64x1_t>(),
        align_of::<uint64x1_t>()
    );
    let bytes = [
        0, 255, 128, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
        23, 24, 25, 26, 27, 28, 29,
    ];
    let mut offset = 0;
    while offset <= 16 {
        unsafe {
            let v = vld1q_u8(bytes.as_ptr().add(offset));
            println!("{:?}", v);
            println!("{:?}", vreinterpretq_u64_u8(v));
            let pair = observe(combine(v, (offset + 3) as u8));
            println!("{} {} {}", offset, pair.0, pair.1);
            // Borrowing and array copies exercise indirect storage as well as calls.
            let copies = [v, combine(v, 255)];
            let saved = &copies[1];
            let pair = observe(*saved);
            println!("{} {}", pair.0, pair.1);
            let halves = vreinterpretq_u16_u8(v);
            let half_ptr = &halves as *const uint16x8_t as *const u16;
            println!("{} {}", *half_ptr, *half_ptr.add(7));
            let small = *(&v as *const uint8x16_t as *const uint8x8_t);
            println!("{}", vget_lane_u64::<0>(vreinterpret_u64_u8(small)));
        }
        offset += 1;
    }
}
