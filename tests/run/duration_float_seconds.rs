use std::time::{Duration, TryFromFloatSecsError};

const SECONDS: f32 = Duration::new(2, 250_000_000).as_secs_f32();

fn print_result(value: f64) {
    let result: Result<Duration, TryFromFloatSecsError> = Duration::try_from_secs_f64(value);
    match result {
        Ok(duration) => println!("{} {}", duration.as_secs(), duration.subsec_nanos()),
        Err(error) => println!("{error}"),
    }
}

fn main() {
    println!("{}", SECONDS.to_bits());
    for (secs, nanos) in [
        (0, 1),
        (1, 999_999_999),
        (16_777_217, 500_000_000),
        (u64::MAX, 0),
    ] {
        println!("{}", Duration::new(secs, nanos).as_secs_f32().to_bits());
    }
    for bits in [
        0,
        0x8000_0000_0000_0000,
        1,
        (0.499_999_999e-9f64).to_bits(),
        (0.500_000_001e-9f64).to_bits(),
        (1.0f64 / 1024.0).to_bits(),
        (3.0f64 / 1024.0).to_bits(),
        1.0f64.to_bits() - 1,
        (8.0f64 + 3.0 / 1024.0).to_bits(),
        0x43ef_ffff_ffff_ffff,
        0x4400_0000_0000_0000,
        (-1.0f64).to_bits(),
        f64::INFINITY.to_bits(),
        f64::NEG_INFINITY.to_bits(),
        f64::NAN.to_bits(),
    ] {
        print_result(f64::from_bits(bits));
    }

    for value in [-0.0, 2.25f32, 1.0 / 1024.0, 3.0 / 1024.0, 1.0e-8, 1.0e18] {
        let duration = Duration::from_secs_f32(value);
        assert_eq!(Duration::try_from_secs_f32(value), Ok(duration));
        assert_eq!(Duration::from_secs_f64(value as f64), duration);
        println!("{} {}", duration.as_secs(), duration.subsec_nanos());
    }
    for value in [-1.0, f32::INFINITY, f32::NAN] {
        println!("{}", Duration::try_from_secs_f32(value).unwrap_err());
    }
}
