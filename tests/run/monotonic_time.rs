use std::time::{Duration, Instant};

const NORMALIZED: Duration = Duration::new(7, 2_500_000_123);
const SUM: Option<Duration> = Duration::new(1, 999_999_999).checked_add(Duration::from_nanos(2));

fn main() {
    assert!(NORMALIZED == Duration::new(9, 500_000_123));
    assert!(SUM == Some(Duration::new(2, 1)));
    assert!(Duration::ZERO.is_zero());
    assert!(Duration::default().is_zero());
    assert!(
        Duration::new(0, 1)
            .checked_sub(Duration::new(0, 2))
            .is_none()
    );
    assert!(Duration::MAX.checked_add(Duration::from_nanos(1)).is_none());
    assert!(Duration::MAX.checked_add(Duration::MAX).is_none());
    assert!(Duration::new(9, 3) - Duration::new(4, 5) == Duration::new(4, 999_999_998));
    let units = Duration::from_secs(1)
        + Duration::from_millis(250)
        + Duration::from_micros(500)
        + Duration::from_nanos(123);
    assert!(units.as_secs() == 1 && units.subsec_nanos() == 250_500_123);
    assert!(units.as_nanos() == 1_250_500_123);
    assert!(units.as_micros() == 1_250_500 && units.as_millis() == 1250);
    assert!((units.as_secs_f64() - 1.250500123).abs() < 1e-15);
    assert!(Duration::MAX.as_nanos() == u64::MAX as u128 * 1_000_000_000 + 999_999_999);

    let mut float_digest = 0u64;
    for secs in [0, 1, 9, (1u64 << 32) - 1, 1u64 << 53, u64::MAX] {
        for nanos in [0, 1, 250_500_123, 499_999_999, 500_000_000, 999_999_999] {
            float_digest = float_digest
                .wrapping_mul(31)
                .wrapping_add(Duration::new(secs, nanos).as_secs_f64().to_bits());
        }
    }
    println!("seconds bits {}", float_digest);

    let start = Instant::now();
    let future = start + Duration::new(2, 4);
    assert!(future.duration_since(start) == Duration::new(2, 4));
    assert!(future - start == Duration::new(2, 4));
    assert!(future - Duration::new(2, 4) == start);
    assert!(start.checked_duration_since(future).is_none());
    assert!(start.duration_since(future).is_zero());
    assert!(start.saturating_duration_since(future).is_zero());
    assert!(start.checked_add(Duration::MAX).is_none());
    assert!(start.checked_sub(Duration::MAX).is_none());
    let before_epoch = start
        .checked_sub(Duration::from_secs(10_000_000_000))
        .unwrap();
    assert!(start - before_epoch == Duration::from_secs(10_000_000_000));
    let selected = true.then(Instant::now).unwrap();
    let mut last = selected;
    for _ in 0..4096 {
        let now = Instant::now();
        assert!(now >= last);
        last = now;
    }
    assert!(last >= start);
    assert!(start.elapsed().as_secs_f64() >= 0.0);
    println!(
        "duration {} {} monotonic",
        units.as_nanos(),
        NORMALIZED.as_nanos()
    );
}
