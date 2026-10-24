use core::sync::atomic::{AtomicPtr, Ordering};
use core::{mem, ptr};

const EMPTY: AtomicPtr<u64> = AtomicPtr::new(ptr::null_mut());
static PUBLISHED: AtomicPtr<&'static str> = AtomicPtr::new(ptr::null_mut());
static TEXT: &str = "published pointer to a sized reference";
static INITIAL: AtomicPtr<&'static str> = AtomicPtr::new(ptr::from_ref(&TEXT).cast_mut());

fn markers<T: Send + Sync>() {}
fn main() {
    markers::<AtomicPtr<core::cell::Cell<u8>>>();
    markers::<AtomicPtr<*mut u8>>();
    println!(
        "{} {}",
        mem::size_of::<AtomicPtr<u64>>(),
        mem::align_of::<AtomicPtr<u64>>()
    );
    println!(
        "{}",
        INITIAL.load(Ordering::Acquire) == ptr::from_ref(&TEXT).cast_mut()
    );
    let mut a = EMPTY;
    let mut x = 41u64;
    let p = ptr::from_mut(&mut x);
    println!("{}", a.load(Ordering::Relaxed).is_null());
    a.store(p, Ordering::Relaxed);
    println!("{}", a.load(Ordering::Relaxed) == p);
    a.store(p, Ordering::Release);
    println!("{}", a.load(Ordering::Acquire) == p);
    a.store(p, Ordering::SeqCst);
    println!("{}", a.load(Ordering::SeqCst) == p);
    // SAFETY: x is live and no access overlaps this read.
    println!("{}", unsafe { *a.load(Ordering::Acquire) });
    *a.get_mut() = ptr::null_mut();
    println!("{}", a.into_inner().is_null());
    let mut z = ();
    let zp = ptr::from_mut(&mut z);
    let zst = AtomicPtr::new(zp);
    println!("{}", zst.load(Ordering::Acquire) == zp);
    PUBLISHED.store(ptr::from_ref(&TEXT).cast_mut(), Ordering::Release);
    let slot = PUBLISHED.load(Ordering::Acquire);
    println!("{}", slot == ptr::from_ref(&TEXT).cast_mut());
    // SAFETY: published storage is a live static reference, never mutated.
    println!("{}", unsafe { *slot });
}
