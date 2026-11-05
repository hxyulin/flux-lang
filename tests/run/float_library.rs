// Float math from the system math library, bit-level float methods, and
// the legacy numeric modules and constants.
#![allow(deprecated)]
use std::f32::consts as c32;
use std::f64::consts::{self, PI};

fn main() {
    let samples = [0.0f64, -0.0, 0.5, -0.5, 1.5, 2.5, -2.5, 0.7, 3.25, 1e300, f64::INFINITY, f64::NAN, -7.9];
    for x in samples {
        println!("{x:?}: {:?} {:?} {:?} {:?} {:?} {:?}", x.round(), x.fract(), x.signum(), x.to_degrees(), x.to_radians(), x.to_le_bytes());
        println!("  {:?} {:?} {:?} {:?} {:?} {:?} {:?}", x.exp(), x.ln(), x.log2(), x.log10(), x.sin(), x.cos(), x.tan());
        println!("  {:?} {:?} {:?} {:?} {:?} {:?}", x.atan(), x.sinh(), x.tanh(), x.asinh(), x.cbrt(), x.exp_m1());
        println!("  {:?} {:?} {:?} {:?}", x.powf(1.5), x.hypot(3.0), x.atan2(-1.0), x.rem_euclid(2.0));
    }
    for x in [0.0f32, -1.5, 0.25, 2.5, 100.0, f32::NAN, -3.75] {
        println!("{x:?}: {:?} {:?} {:?} {:?} {:?} {:?} {:?}", x.round(), x.fract(), x.signum(), x.to_degrees(), x.to_radians(), x.to_be_bytes(), x.sin_cos());
        println!("  {:?} {:?} {:?} {:?} {:?} {:?}", x.exp2(), x.ln_1p(), x.acos(), x.asin(), x.acosh(), x.atanh());
        println!("  {:?} {:?} {:?} {:?}", x.powf(0.5), x.log(3.0), x.div_euclid(-2.0), x.abs_sub(1.0));
    }
    println!("{:?} {:?}", f32::from_le_bytes([0, 0, 128, 63]), f64::from_be_bytes([64, 9, 33, 251, 84, 68, 45, 24]));
    println!("{:?} {:?}", f32::from_ne_bytes(1.5f32.to_ne_bytes()), f64::from_le_bytes(PI.to_le_bytes()));
    let mut ordered = [3.0, -0.0, f64::NAN, 0.0, -f64::INFINITY, -1.0];
    ordered.sort_by(|a, b| a.total_cmp(b));
    println!("{:?}", ordered);
    println!("{:?} {:?} {:?}", 5.0f64.clamp(0.0, 2.0), (-1.0f32).clamp(0.0, 2.0), f64::NAN.clamp(0.0, 1.0));
    println!("{:?} {:?}", 1.0f64.midpoint(4.0), f64::MAX.midpoint(f64::MAX));
    println!("{} {} {} {} {}", PI, consts::E, consts::LN_2, consts::SQRT_2, consts::FRAC_1_SQRT_2);
    println!("{} {} {} {}", c32::PI, c32::TAU, c32::LOG10_E, c32::FRAC_PI_6);
    println!("{} {} {} {}", std::f32::MANTISSA_DIGITS, std::f64::MANTISSA_DIGITS, std::f64::MAX_10_EXP, core::f32::RADIX);
    println!("{} {} {} {}", core::i8::MIN, core::i16::MAX, core::i32::MIN, core::i64::MAX);
    println!("{} {} {} {}", core::u16::MAX, core::u64::MAX, core::u128::MAX, core::i128::MIN);
    println!("{} {} {}", std::usize::MAX, std::isize::MIN, std::i32::MAX);
}
