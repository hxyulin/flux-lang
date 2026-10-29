use core::time::Duration;
use std::time::Duration as Std;
const NORMALIZED: Duration = Duration::new(7, 2_500_000_123);
const SUM: Option<Duration> = Duration::new(1, 999_999_999).checked_add(Duration::from_nanos(2));
fn hash(mut state: u64, text: &str) -> u64 {
    for byte in text.as_bytes() {
        state = (state ^ (*byte as u64)).wrapping_mul(1099511628211);
    }
    state
}
fn identity(value: Std) -> Duration {
    value
}
fn main() {
    assert_eq!(identity(NORMALIZED), Std::new(9, 500_000_123));
    assert_eq!(SUM, Some(Std::new(2, 1)));
    assert_eq!(Duration::default(), Std::ZERO);
    assert_eq!(Duration::MAX.checked_add(Std::from_nanos(1)), None);
    assert_eq!(Duration::ZERO.checked_sub(Std::from_nanos(1)), None);
    assert_eq!(
        Duration::new(4, 5) - Std::new(2, 7),
        Duration::new(1, 999_999_998)
    );
    let mut state = 1469598103934665603u64;
    let mut cases = 0;
    for secs in [0, 1, u64::MAX] {
        for nanos in [
            0,
            1,
            999,
            1000,
            1001,
            1499,
            1500,
            1999,
            999_999,
            1_000_000,
            1_000_001,
            999_499_999,
            999_500_000,
            999_999_999,
        ] {
            let d = Duration::new(secs, nanos);
            state = hash(state, &format!("{:?}", d));
            cases += 1;
            for precision in [0, 1, 2, 3, 6, 9, 10, 12, 64, 128] {
                for width in [0, 3, 20, 48] {
                    state = hash(state, &format!("{d:.precision$?}"));
                    state = hash(state, &format!("{d:多>+width$.precision$?}"));
                    state = hash(state, &format!("{d:🦀^width$.precision$?}"));
                    state = hash(state, &format!("{d:0width$.precision$?}"));
                    cases += 4;
                }
            }
        }
    }
    println!("core duration {} {} {:?}", cases, state, Duration::MAX);
}
