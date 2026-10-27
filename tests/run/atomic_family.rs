use Ordering::*;
use std::sync::atomic::*;
fn traits<T: Send + Sync>() {}
macro_rules! signed {
    ($atomic:ident,$integer:ty,$size:literal) => {{
        assert_eq!(std::mem::size_of::<$atomic>(), $size);
        assert_eq!(std::mem::align_of::<$atomic>(), $size);
        traits::<$atomic>();
        let mut value = $atomic::new(<$integer>::MIN);
        for n in [<$integer>::MIN, -1, 0, 1, <$integer>::MAX] {
            value.store(n, Relaxed);
            assert_eq!(value.load(Relaxed), n);
            value.store(n, Release);
            assert_eq!(value.load(Acquire), n);
            value.store(n, SeqCst);
            assert_eq!(value.load(SeqCst), n);
        }
        *value.get_mut() = -17;
        assert_eq!(value.into_inner(), -17);
        assert_eq!($atomic::default().into_inner(), 0);
        assert_eq!(format!("{:?}", $atomic::from(-9)), "-9");
    }};
}
#[repr(C)]
struct Guard {
    before: u16,
    value: AtomicU16,
    after: u16,
}
fn main() {
    signed!(AtomicI8, i8, 1);
    signed!(AtomicI16, i16, 2);
    signed!(AtomicI32, i32, 4);
    signed!(AtomicI64, i64, 8);
    signed!(AtomicIsize, isize, 8);
    assert_eq!(std::mem::size_of::<AtomicU16>(), 2);
    assert_eq!(std::mem::align_of::<AtomicU16>(), 2);
    assert_eq!(std::mem::size_of::<Guard>(), 6);
    traits::<AtomicU16>();
    let guard = Guard {
        before: 0xabcd,
        value: AtomicU16::new(0),
        after: 0xef12,
    };
    for n in [0, 1, 255, 256, 32767, 32768, 65535] {
        guard.value.store(n, Relaxed);
        assert_eq!(guard.value.load(Relaxed), n);
        guard.value.store(n, Release);
        assert_eq!(guard.value.load(Acquire), n);
        guard.value.store(n, SeqCst);
        assert_eq!(guard.value.load(SeqCst), n);
        assert_eq!(guard.before, 0xabcd);
        assert_eq!(guard.after, 0xef12);
    }
    assert_eq!(std::mem::size_of::<AtomicBool>(), 1);
    assert_eq!(std::mem::align_of::<AtomicBool>(), 1);
    traits::<AtomicBool>();
    let mut flag = AtomicBool::new(false);
    flag.store(true, Release);
    assert!(flag.load(Acquire));
    flag.store(false, SeqCst);
    assert!(!flag.load(SeqCst));
    *flag.get_mut() = true;
    assert!(flag.into_inner());
    assert!(!AtomicBool::default().into_inner());
    assert_eq!(format!("{:?}", AtomicBool::from(true)), "true");
    println!("atomic family signed 1 2 4 8 8 unsigned 2 bool 1");
}
