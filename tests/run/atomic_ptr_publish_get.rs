use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

// Standalone publication shape only: no allocator, locks, TLS or hash table.
struct PublishedStrings {
    bucket: [AtomicPtr<&'static str>; 2],
}
impl PublishedStrings {
    const fn new() -> Self {
        Self {
            bucket: [
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
            ],
        }
    }
    fn publish(&self, slot: usize, s: &'static &'static str) {
        self.bucket[slot].store(ptr::from_ref(s).cast_mut(), Ordering::Release);
    }
    fn get(&self, slot: usize) -> Option<&'static str> {
        let p = self.bucket[slot].load(Ordering::Acquire);
        if p.is_null() {
            None
        } else {
            // SAFETY: publish accepts only static immutable reference storage.
            Some(unsafe { *p })
        }
    }
}
static TABLE: PublishedStrings = PublishedStrings::new();
static FIRST: &str = "first interned string";
static SECOND: &str = "second interned string";
fn main() {
    println!("{} {}", TABLE.get(0).is_none(), TABLE.get(1).is_none());
    TABLE.publish(0, &FIRST);
    TABLE.publish(1, &SECOND);
    println!("{} {}", TABLE.get(0).unwrap(), TABLE.get(1).unwrap());
    let a = TABLE.bucket[0].load(Ordering::Acquire);
    let b = TABLE.bucket[1].load(Ordering::Acquire);
    println!(
        "{} {}",
        a == ptr::from_ref(&FIRST).cast_mut(),
        b == ptr::from_ref(&SECOND).cast_mut()
    );
}
