fn main() {
    for value in [
        0.0,
        -0.0,
        2.5,
        3.5,
        1.25,
        -1.75,
        9.999,
        0.09999,
        12_345.625,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
    ] {
        println!("{value:e} {value:E} {value:.0e} {value:.1E} {value:.6e}");
        println!("{value:+018.4E} |{value:*^17.2e}| {value:#.0E}");
    }
    for value in [
        0.0f32,
        -0.0,
        2.5,
        3.5,
        1.25,
        -1.75,
        9.999,
        0.09999,
        12_345.625,
        f32::from_bits(1),
        f32::MIN_POSITIVE,
        f32::MAX,
        f32::INFINITY,
        f32::NEG_INFINITY,
        f32::NAN,
    ] {
        println!("{value:e} {value:E} {value:.0e} {value:.1E} {value:.16e}");
        println!("{value:+018.4E} |{value:*<17.2e}| {value:#.0E}");
    }
    let mut bits = 37u64;
    for precision in 0..64 {
        bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
        let value = f64::from_bits(bits);
        println!("{value:e} {value:.precision$E}");
        let short = f32::from_bits((bits >> 31) as u32);
        println!("{short:E} {short:.precision$e}");
    }
    println!("{:.800e} {:.800E}", f64::from_bits(1), -0.0f64);
}
