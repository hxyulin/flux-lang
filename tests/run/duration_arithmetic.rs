// Duration operators and saturating/checked arithmetic.
use std::time::Duration;

fn main() {
    let mut total = Duration::ZERO;
    for millis in [250, 750, 1_500] {
        total += Duration::from_millis(millis);
    }
    println!("{total:?} {}", total.as_millis());
    total -= Duration::from_micros(500_001);
    println!("{total:?} {} {}", total.as_secs(), total.subsec_nanos());
    let sum = Duration::new(1, 999_999_999) + Duration::new(0, 1);
    let difference = Duration::new(3, 0) - Duration::new(1, 500_000_000);
    println!("{sum:?} {difference:?}");
    println!("{:?}", Duration::from_secs(1).saturating_sub(Duration::from_secs(2)));
    println!("{:?}", Duration::from_secs(5).saturating_sub(Duration::from_millis(1)));
    println!("{}", Duration::MAX.saturating_add(Duration::from_secs(1)) == Duration::MAX);
    println!("{:?}", Duration::from_secs(1).saturating_add(Duration::from_nanos(7)));
    println!("{:?}", Duration::from_secs(1).checked_sub(Duration::from_secs(2)));
    println!("{:?}", Duration::from_secs(2).checked_sub(Duration::from_nanos(1)));
    println!("{:?}", Duration::MAX.checked_add(Duration::from_nanos(1)));
}
