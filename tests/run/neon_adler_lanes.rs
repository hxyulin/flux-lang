#[cfg(target_arch = "aarch64")]
fn main() {
    use std::arch::aarch64::*;

    let source: [u8; 65] = std::array::from_fn(|i| i as u8);
    let words = [2u16; 4];
    // SAFETY: NEON is baseline on AArch64, the load ranges are initialized,
    // the immediate is valid, and the reinterpreted types accept every bit.
    unsafe {
        let start = source.as_ptr().add(1);
        let vectors = vld1q_u8_x4(start);
        let consecutive: [u8; 16] = std::mem::transmute(vectors.3);
        assert_eq!(consecutive.as_slice(), &source[49..]);
        let channels = vld4q_u8(start);
        let channel0: [u8; 16] = std::mem::transmute(channels.0);
        let channel3: [u8; 16] = std::mem::transmute(channels.3);
        assert_eq!(channel0, std::array::from_fn(|i| (4 * i + 1) as u8));
        assert_eq!(channel3, std::array::from_fn(|i| (4 * i + 4) as u8));

        let bytes = vdupq_n_u8(7);
        let sum = vpaddlq_u8(bytes);
        let pairwise: [u32; 4] =
            std::mem::transmute(vpadalq_u16(vdupq_n_u32(5), vpadalq_u8(sum, bytes)));
        assert_eq!(pairwise, [61; 4]);
        let wide = vdupq_n_u16(u16::MAX);
        let lower: [u16; 8] = std::mem::transmute(vaddw_u8(wide, vget_low_u8(bytes)));
        let upper: [u16; 8] = std::mem::transmute(vaddw_high_u8(wide, bytes));
        assert_eq!(lower, [6; 8]);
        assert_eq!(upper, lower);
        let short = vld1_u16(words.as_ptr());
        let low: [u32; 4] = std::mem::transmute(vmlal_u16(vdupq_n_u32(1), short, short));
        let high: [u32; 4] = std::mem::transmute(vmlal_high_u16(
            vdupq_n_u32(1),
            vdupq_n_u16(2),
            vdupq_n_u16(2),
        ));
        assert_eq!(low, [5; 4]);
        assert_eq!(high, low);
        let pair = vpadd_u32(vget_low_u32(vdupq_n_u32(3)), vget_low_u32(vdupq_n_u32(4)));
        assert_eq!([vget_lane_u32::<0>(pair), vget_lane_u32::<1>(pair)], [6, 8]);

        let reversed: [u8; 16] = std::mem::transmute(vrev16q_u8(vectors.0));
        assert_eq!(reversed, std::array::from_fn(|i| ((i ^ 1) + 1) as u8));
        let merged: [u8; 16] =
            std::mem::transmute(vsriq_n_u8::<4>(vdupq_n_u8(0xaa), vdupq_n_u8(0xf0)));
        assert_eq!(merged, [0xaf; 16]);
        println!("adler lanes passed");
    }
}

#[cfg(not(target_arch = "aarch64"))]
fn main() {}
