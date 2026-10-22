use core::arch::aarch64::*;
use core::arch::aarch64::{vgetq_lane_u64 as lane, vshrn_n_u16 as narrow};

const ZERO: i32 = 0;
mod constants {
    pub const SHIFT: i32 = 4;
    pub const LAST: i32 = 1;
}

macro_rules! legacy_lane {
    ($a:expr, $n:expr) => { lane($a, $n) };
}

macro_rules! imported_legacy_lane {
    ($a:expr) => {{
        use core::arch::aarch64::vgetq_lane_u64 as imported_lane;
        imported_lane($a, 1)
    }};
}

#[inline(never)]
fn generic<const N: i32>(a: uint16x8_t) -> uint8x8_t {
    unsafe { narrow(a, N) }
}

#[inline(never)]
fn generic_lane<const N: i32>(a: uint64x2_t) -> u64 {
    unsafe { lane(a, N) }
}

fn invoke<F: Fn(uint16x8_t) -> uint8x8_t>(f: F, a: uint16x8_t) -> uint8x8_t {
    f(a)
}

#[inline(never)]
fn observe(input: [u8; 16]) {
    unsafe {
        let bytes = vld1q_u8(input.as_ptr());
        let words = vreinterpretq_u64_u8(bytes);
        println!("imported={}", imported_legacy_lane!(words));
        println!("{} {} {} {} {}", lane(words, ZERO), lane(words, constants::LAST),
            legacy_lane!(words, (0)), lane(words, -0i32), generic_lane::<1>(words));
        let halves = vreinterpretq_u16_u8(bytes);
        let pointer: fn(uint16x8_t) -> uint8x8_t = generic::<4>;
        let modern: unsafe fn(uint16x8_t) -> uint8x8_t = vshrn_n_u16::<4>;
        let item = vgetq_lane_u64::<1>;
        println!("{}", item(words));
        let results = [narrow(halves, 4), narrow(halves, (constants::SHIFT)),
            generic::<4>(halves), pointer(halves), invoke(generic::<4>, halves), modern(halves)];
        for result in results {
            println!("{} {}", vget_lane_u64(vreinterpret_u64_u8(result), 0),
                vget_lane_u64::<0>(vreinterpret_u64_u8(result)));
        }
        // A user function with the same name retains ordinary runtime arguments.
        println!("{}", user::vget_lane_u64(lane(words, 0), input[0] as u64));
    }
}

mod user {
    pub fn vget_lane_u64(a: u64, n: u64) -> u64 { a.wrapping_add(n) }
}

fn main() {
    observe([0, 1, 2, 3, 128, 255, 127, 64, 255, 254, 253, 252, 16, 32, 48, 64]);
    observe([255; 16]);
}
