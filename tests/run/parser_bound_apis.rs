use std::ops::{Bound, RangeBounds};
use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
struct Key(u32);
impl Drop for Key {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
fn bounds<T: ?Sized, R: RangeBounds<T>>(r: &R) -> (Bound<&T>, Bound<&T>) {
    (r.start_bound(), r.end_bound())
}
fn require_error<E: std::error::Error + ToString + Send + Sync + 'static>() {}
#[derive(Debug)]
struct Owned(String);
impl Drop for Owned {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Debug)]
struct Rejected(Owned);
impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0.0)
    }
}
impl std::error::Error for Rejected {}
impl TryFrom<Owned> for u8 {
    type Error = Rejected;
    fn try_from(value: Owned) -> Result<Self, Self::Error> {
        Err(Rejected(value))
    }
}
fn main() {
    require_error::<core::convert::Infallible>();
    require_error::<<i64 as TryFrom<i64>>::Error>();
    require_error::<<u8 as TryFrom<i64>>::Error>();
    assert_eq!(i64::try_from(37i64).unwrap(), 37);
    assert_eq!(u8::try_from(255i64).unwrap(), 255);
    for value in [-1i64, 256, i64::MAX] {
        let e = u8::try_from(value).unwrap_err();
        let obj: &dyn std::error::Error = &e;
        assert!(obj.source().is_none());
        assert_eq!(
            e.to_string(),
            if value < 0 {
                "number too small to fit in target type"
            } else {
                "number too large to fit in target type"
            }
        );
        println!("conversion {} {} {:?}", value, obj, e);
    }
    let a = 7i64;
    let b = 19i64;
    let range = &a..=&b;
    let (lo, hi) = bounds::<i64, _>(&range);
    assert!(matches!(lo, Bound::Included(7)));
    assert!(matches!(hi, Bound::Included(19)));
    let range = &a..&b;
    let (lo, hi) = bounds::<i64, _>(&range);
    assert!(matches!(lo, Bound::Included(7)));
    assert!(matches!(hi, Bound::Excluded(19)));
    assert!(matches!(bounds::<i64, _>(&(&a..)).1, Bound::Unbounded));
    assert!(matches!(bounds::<i64, _>(&(..&b)).0, Bound::Unbounded));
    assert!(matches!(bounds::<i64, _>(&(..=&b)).1, Bound::Included(19)));
    {
        let a = Key(3);
        let b = Key(9);
        let range = &a..=&b;
        let (lo, hi) = bounds::<Key, _>(&range);
        let Bound::Included(lo) = lo else { panic!() };
        let Bound::Included(hi) = hi else { panic!() };
        assert!(std::ptr::eq(lo, &a));
        assert!(std::ptr::eq(hi, &b));
        assert_eq!(lo.0 + hi.0, 12);
        assert_eq!(DROPS.load(Ordering::SeqCst), 0);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 2);
    {
        let rejected = u8::try_from(Owned(String::from("owned rejection"))).unwrap_err();
        assert_eq!(DROPS.load(Ordering::SeqCst), 2);
        let error: &dyn std::error::Error = &rejected;
        assert_eq!(error.to_string(), "owned rejection");
        assert!(error.source().is_none());
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 3);

    let range = (Bound::Included("alpha"), Bound::Excluded("omega"));
    let (lo, hi) = bounds::<str, _>(&range);
    assert_eq!(lo, Bound::Included("alpha"));
    assert_eq!(hi, Bound::Excluded("omega"));
    println!("bounds {} {} drops {}", a, b, DROPS.load(Ordering::SeqCst));
}
