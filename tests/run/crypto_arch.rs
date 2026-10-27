use core::arch::aarch64::*;

fn next(state: &mut u64) -> u64 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 7;
    x ^= x << 17;
    *state = x;
    x
}
fn digest(hash: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *hash = (*hash ^ *byte as u64).wrapping_mul(0x100000001b3);
    }
}
unsafe fn words32(v: uint32x4_t) -> [u32; 4] {
    let mut out = [0; 4];
    unsafe {
        vst1q_u32(out.as_mut_ptr(), v);
    }
    out
}
unsafe fn words64(v: uint64x2_t) -> [u64; 2] {
    let mut out = [0; 2];
    unsafe {
        vst1q_u64(out.as_mut_ptr(), v);
    }
    out
}
fn hash32(hash: &mut u64, v: [u32; 4]) {
    for x in v {
        digest(hash, &x.to_le_bytes());
    }
}
fn hash64(hash: &mut u64, v: [u64; 2]) {
    for x in v {
        digest(hash, &x.to_le_bytes());
    }
}
fn main() {
    assert_eq!(core::mem::size_of::<uint32x4_t>(), 16);
    assert_eq!(core::mem::align_of::<uint32x4_t>(), 16);
    let mut random = 0x8e31_052a_a782_1f49;
    let mut hash = 0xcbf29ce484222325;
    unsafe {
        for _ in 0..1024 {
            let a32 = [
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
            ];
            let b32 = [
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
            ];
            let c32 = [
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
                next(&mut random) as u32,
            ];
            let a = vld1q_u32(a32.as_ptr());
            let b = vld1q_u32(b32.as_ptr());
            let c = vld1q_u32(c32.as_ptr());
            hash32(&mut hash, words32(vaddq_u32(a, b)));
            hash32(&mut hash, words32(vsha256hq_u32(a, b, c)));
            hash32(&mut hash, words32(vsha256h2q_u32(a, b, c)));
            hash32(&mut hash, words32(vsha256su0q_u32(a, b)));
            hash32(&mut hash, words32(vsha256su1q_u32(a, b, c)));
            let a64 = [next(&mut random), next(&mut random)];
            let b64 = [next(&mut random), next(&mut random)];
            let c64 = [next(&mut random), next(&mut random)];
            let a = vld1q_u64(a64.as_ptr());
            let b = vld1q_u64(b64.as_ptr());
            let c = vld1q_u64(c64.as_ptr());
            assert_eq!(words64(vextq_u64::<0>(a, b)), a64);
            assert_eq!(words64(vextq_u64(a, b, 1)), [a64[1], b64[0]]);
            hash64(&mut hash, words64(vaddq_u64(a, b)));
            hash64(&mut hash, words64(vsha512hq_u64(a, b, c)));
            hash64(&mut hash, words64(vsha512h2q_u64(a, b, c)));
            hash64(&mut hash, words64(vsha512su0q_u64(a, b)));
            hash64(&mut hash, words64(vsha512su1q_u64(a, b, c)));
        }
        for offset in 0..16 {
            let input: [u8; 48] = core::array::from_fn(|i| (i * 7) as u8);
            let bytes = vld1q_u8(input.as_ptr().add(offset));
            hash32(&mut hash, words32(vreinterpretq_u32_u8(vrev32q_u8(bytes))));
            hash64(&mut hash, words64(vreinterpretq_u64_u8(vrev64q_u8(bytes))));
            let a = vld1q_u32(input.as_ptr().add(offset) as *const u32);
            let b = vld1q_u64(input.as_ptr().add(offset) as *const u64);
            let mut output = [0xa5u8; 48];
            vst1q_u32(output.as_mut_ptr().add(offset) as *mut u32, a);
            assert_eq!(&output[offset..offset + 16], &input[offset..offset + 16]);
            assert!(output[..offset].iter().all(|x| *x == 0xa5));
            assert!(output[offset + 16..].iter().all(|x| *x == 0xa5));
            output = [0xa5; 48];
            vst1q_u64(output.as_mut_ptr().add(offset) as *mut u64, b);
            assert_eq!(&output[offset..offset + 16], &input[offset..offset + 16]);
            assert!(output[..offset].iter().all(|x| *x == 0xa5));
            assert!(output[offset + 16..].iter().all(|x| *x == 0xa5));
        }
        let one = [0u64, u64::MAX];
        let two = [1, u64::MAX];
        let a = vld1q_u64(one.as_ptr());
        let b = vld1q_u64(two.as_ptr());
        let add: unsafe fn(uint64x2_t, uint64x2_t) -> uint64x2_t = vaddq_u64;
        assert_eq!(words64(add(a, b)), [1, u64::MAX - 1]);
    }
    println!("crypto lanes {}", hash);
}
