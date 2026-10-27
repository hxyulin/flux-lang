use std::time::{SystemTime,UNIX_EPOCH,Duration};
fn main() {
    let before=UNIX_EPOCH-Duration::from_nanos(1);
    assert_eq!(before.duration_since(UNIX_EPOCH).unwrap_err().duration(),Duration::from_nanos(1));
    assert_eq!(UNIX_EPOCH.duration_since(before).unwrap(),Duration::from_nanos(1));
    assert_eq!(before+Duration::from_nanos(1),UNIX_EPOCH);
    let mut t=UNIX_EPOCH+Duration::new(7,999999999);t+=Duration::from_nanos(1);
    assert_eq!(t.duration_since(UNIX_EPOCH).unwrap(),Duration::from_secs(8));
    t-=Duration::from_secs(9);assert!(t<UNIX_EPOCH);
    assert!(UNIX_EPOCH.checked_add(Duration::MAX).is_none());
    assert!(UNIX_EPOCH.checked_sub(Duration::MAX).is_none());
    for duration in [Duration::ZERO,Duration::from_nanos(1),Duration::from_nanos(2500),Duration::from_nanos(1500000),Duration::new(1,999999999),Duration::MAX] {
        println!("duration {:?}|{:.0?}|{:.1?}|{:.3?}|{:.12?}|{:+15.3?}|{:多^18.2?}",duration,duration,duration,duration,duration,duration,duration);
    }
    let now=SystemTime::now();assert!(now.duration_since(UNIX_EPOCH).unwrap().as_secs()>1700000000);
    assert!(SystemTime::UNIX_EPOCH.elapsed().unwrap().as_secs()>1700000000);
    println!("system time {:?} {:?} {}",before,t,before.duration_since(UNIX_EPOCH).unwrap_err());
}
