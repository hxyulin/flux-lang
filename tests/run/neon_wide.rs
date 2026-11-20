use core::{arch::aarch64::*, mem::transmute};

fn main() {
    let left = [2.0_f64, 3.0];
    let right = [4.0_f64, 5.0];
    let mut matrix = [0.0_f64; 5];
    let mut singles = [0.0_f32; 2];
    // SAFETY: the input and output regions cover every loaded or stored lane.
    unsafe {
        let a = vld1q_f64(left.as_ptr());
        let b = vld1q_f64(right.as_ptr());
        let c = vld1q_dup_f64(left.as_ptr());
        let d = vld1q_lane_f64::<0>(right.as_ptr().add(1), c);
        let sum = vfmaq_f64(vmovq_n_f64(1.0), a, b);
        let sum = vfmaq_laneq_f64::<0>(sum, d, b);
        vst1q_f64(matrix.as_mut_ptr(), sum);
        vst1q_lane_f64::<1>(matrix.as_mut_ptr().add(2), vmulq_n_f64(sum, 2.0));

        let x = 1.0 + 0.000000007450580596923828125_f64;
        let y = 1.0 - 0.000000007450580596923828125_f64;
        let fused = vfmaq_f64(vmovq_n_f64(-1.0), vmovq_n_f64(x), vmovq_n_f64(y));
        vst1q_f64(matrix.as_mut_ptr().add(3), fused);

        let lane = 3.0_f32;
        let padded = vld1q_lane_f32::<2>(&lane, vmovq_n_f32(2.0));
        let fused = vfmaq_laneq_f32::<2>(padded, vmovq_n_f32(4.0), padded);
        vst1q_lane_f32::<2>(singles.as_mut_ptr(), fused);
        vst1q_lane_f32::<0>(singles.as_mut_ptr().add(1), fused);
    }
    println!("{:?}", matrix.map(f64::to_bits));
    println!("{:?}", singles.map(f32::to_bits));
    // SAFETY: the host provides NEON and all lanes are initialized above.
    unsafe {
        compare_wide_lanes();
    }
}

#[target_feature(enable = "neon")]
fn compare_wide_lanes() {
    // SAFETY: the arrays have identical storage to their vector types.
    let signed: int16x8_t =
        unsafe { transmute([i16::MIN, i16::MAX, -129, -128, 127, 128, 31, -32]) };
    let saturated = vqaddq_s16(signed, vmovq_n_s16(100));
    let narrowed = vqmovn_s16(saturated);
    let rounded = vqrdmulhq_n_s16(signed, i16::MIN);
    // SAFETY: the arrays exactly represent the vector's lanes.
    let narrowed: [i8; 8] = unsafe { transmute(narrowed) };
    let rounded: [i16; 8] = unsafe { transmute(rounded) };
    println!(
        "{} {} {:?} {:?}",
        vminvq_s16(signed),
        vmaxvq_s16(signed),
        narrowed,
        rounded
    );

    let table = uint8x16x4_t(
        vmovq_n_u8(11),
        vmovq_n_u8(22),
        vmovq_n_u8(33),
        vmovq_n_u8(44),
    );
    // SAFETY: sixteen bytes exactly fill the index vector.
    let indices = unsafe {
        transmute([
            0_u8, 15, 16, 31, 32, 47, 48, 63, 64, 255, 1, 32, 63, 16, 15, 99,
        ])
    };
    let lookup = vqtbl4q_u8(table, indices);
    let paired = vuzpq_u8(lookup, vmovq_n_u8(99));
    // SAFETY: neither vector has padding.
    let first: [u8; 16] = unsafe { transmute(paired.0) };
    let second: [u8; 16] = unsafe { transmute(paired.1) };
    println!("{:?} {:?}", first, second);

    let a = [f64::NAN, -0.0];
    let b = [4.0, 0.0];
    // SAFETY: both pointers cover two initialized elements.
    let (a, b) = unsafe { (vld1q_f64(a.as_ptr()), vld1q_f64(b.as_ptr())) };
    let minimum = vminnmq_f64(a, b);
    let maximum = vmaxnmq_f64(a, b);
    println!(
        "{:x} {:x} {:x} {:x}",
        vgetq_lane_f64::<0>(minimum).to_bits(),
        vgetq_lane_f64::<1>(minimum).to_bits(),
        vgetq_lane_f64::<0>(maximum).to_bits(),
        vgetq_lane_f64::<1>(maximum).to_bits()
    );
}
