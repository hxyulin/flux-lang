use core::{arch::aarch64::*, mem::transmute, ptr};

union VectorBits {
    lanes: [f32; 4],
    vector: float32x4_t,
}

// SAFETY: both fields occupy sixteen initialized bytes of floating-point lanes.
const A: float32x4_t = unsafe {
    VectorBits {
        lanes: [1.0, -2.0, 3.0, -4.0],
    }
    .vector
};

fn vector(lanes: [f32; 4]) -> float32x4_t {
    // SAFETY: four initialized f32 lanes exactly fill the NEON vector.
    unsafe { transmute(lanes) }
}

fn bits(value: float32x4_t) -> [u32; 4] {
    // SAFETY: both representations contain sixteen initialized bytes.
    unsafe { transmute(value) }
}

fn words(value: uint32x4_t) -> [u32; 4] {
    // SAFETY: a word vector contains four initialized words.
    unsafe { transmute(value) }
}

fn short_bits(value: float32x2_t) -> [u32; 2] {
    // SAFETY: both representations contain eight initialized bytes.
    unsafe { transmute(value) }
}

#[target_feature(enable = "neon")]
fn compare() {
    let a = A;
    let b = vector([8.0, 16.0, 32.0, 64.0]);
    let signs = vector([-0.0, 0.0, -3.0, 2.5]);
    println!("{:?} {:?}", bits(vabsq_f32(signs)), bits(vnegq_f32(signs)));
    println!("{:?} {:?}", bits(vaddq_f32(a, b)), bits(vsubq_f32(a, b)));
    println!("{:?} {:?}", bits(vmulq_f32(a, b)), bits(vdivq_f32(a, b)));
    println!("{:?}", bits(vmulq_n_f32(a, -0.5)));

    let nan = f32::from_bits(0x7fc1_2345);
    let x = vector([nan, -0.0, -2.0, 3.0]);
    let y = vector([7.0, 0.0, 5.0, 3.0]);
    println!("{:?} {:?}", words(vceqq_f32(x, y)), words(vcgeq_f32(x, y)));
    println!("{:?} {:?}", words(vcgtq_f32(x, y)), words(vcleq_f32(x, y)));
    println!("{:?}", words(vcltq_f32(x, y)));
    println!("{:?}", words(vcltq_s32(vdupq_n_s32(-1), vdupq_n_s32(1))));
    let minimum = vminq_f32(x, y);
    let maximum = vmaxq_f32(x, y);
    assert!(vgetq_lane_f32::<0>(minimum).is_nan());
    assert!(vgetq_lane_f32::<0>(maximum).is_nan());
    println!(
        "{:x} {:x}",
        vgetq_lane_f32::<1>(minimum).to_bits(),
        vgetq_lane_f32::<1>(maximum).to_bits()
    );
    println!("{} {}", vminnmvq_f32(x), vmaxnmvq_f32(x));
    println!(
        "{:x}",
        vaddvq_f32(vector([1e20, 1.0, -1e20, 1.0])).to_bits()
    );

    let fractions = vector([-2.5, 1.5, -0.25, 2.5]);
    println!(
        "{:?} {:?}",
        bits(vrndaq_f32(fractions)),
        bits(vrndnq_f32(fractions))
    );
    println!(
        "{:?} {:?}",
        bits(vrndmq_f32(fractions)),
        bits(vrndpq_f32(fractions))
    );
    println!("{:?}", bits(vrndq_f32(fractions)));

    let up = vdupq_n_f32(1.0 + f32::EPSILON);
    let down = vdupq_n_f32(1.0 - f32::EPSILON);
    let negative = vdupq_n_f32(-1.0);
    let positive = vdupq_n_f32(1.0);
    let fused = vfmaq_f32(negative, up, down);
    let separate = vmlaq_f32(negative, up, down);
    assert_ne!(bits(fused), bits(separate));
    println!("{:?} {:?}", bits(fused), bits(separate));
    println!(
        "{:?} {:?}",
        bits(vfmsq_f32(positive, up, down)),
        bits(vmlsq_f32(positive, up, down))
    );
    println!(
        "{:?}",
        bits(vfmaq_lane_f32::<1>(negative, up, vget_low_f32(down)))
    );
    println!(
        "{:?}",
        bits(vmlaq_lane_f32::<0>(negative, up, vget_high_f32(down)))
    );

    println!("{:?} {:?}", bits(vtrn1q_f32(a, b)), bits(vtrn2q_f32(a, b)));
    println!("{:?} {:?}", bits(vuzp1q_f32(a, b)), bits(vuzp2q_f32(a, b)));
    println!("{:?} {:?}", bits(vzip1q_f32(a, b)), bits(vzip2q_f32(a, b)));
    println!(
        "{:?} {:?}",
        bits(vrev64q_f32(a)),
        bits(vextq_f32::<3>(a, b))
    );
    let zipped = vzip2q_u64(vreinterpretq_u64_f32(a), vreinterpretq_u64_f32(b));
    println!("{:?}", bits(vreinterpretq_f32_u64(zipped)));

    let low = vget_low_f32(a);
    let high = vget_high_f32(b);
    println!(
        "{:?} {:?}",
        short_bits(vsub_f32(low, high)),
        short_bits(vmul_lane_f32::<1>(low, high))
    );
    println!(
        "{:?} {:?}",
        bits(vdupq_lane_f32::<1>(high)),
        bits(vdupq_laneq_f32::<3>(a))
    );
    println!(
        "{} {}",
        vget_lane_f32::<0>(low),
        vmuls_laneq_f32::<2>(-0.5, b)
    );
    println!("{:?}", bits(vsetq_lane_f32(17.0, a, 3)));
    let mask = vsetq_lane_u32::<2>(0x8000_ffff, vdupq_n_u32(0xffff_ff00));
    println!(
        "{:?} {:?}",
        words(vmvnq_u32(mask)),
        bits(vbslq_f32(mask, a, b))
    );
    println!(
        "{:?}",
        words(vreinterpretq_u32_f32(vreinterpretq_f32_u32(mask)))
    );
    // SAFETY: the signed vector contains four initialized words.
    let signed: [i32; 4] = unsafe { transmute(vreinterpretq_s32_f32(x)) };
    println!("{signed:?}");

    let mut storage = [0u8; 18];
    // SAFETY: the offset leaves sixteen writable bytes in `storage`; NEON
    // permits an unaligned address for these loads and stores.
    unsafe {
        let unaligned = storage.as_mut_ptr().add(1).cast::<f32>();
        vst1q_f32(unaligned, a);
        assert_eq!(ptr::read_unaligned(unaligned), 1.0);
        println!(
            "{:?} {:?}",
            bits(vld1q_f32(unaligned)),
            bits(vld1q_dup_f32(unaligned.add(3)))
        );
    }
}

fn main() {
    // SAFETY: the fixture runs on an AArch64 host with NEON.
    unsafe { compare() }
}
