use std::collections::VecDeque;
use std::mem::{MaybeUninit, align_of, size_of};
use std::num::NonZeroI8;
use std::ops::Neg;
use std::ptr;
use std::sync::atomic::{AtomicPtr, Ordering};

const WRAPPED: NonZeroI8 = NonZeroI8::MIN.wrapping_neg();
const DANGLING: *mut u32 = ptr::dangling_mut::<u32>();
const NUMERIC: *mut u32 = ptr::without_provenance_mut::<u32>(4096);

struct Payload(u32);

impl Drop for Payload {
    fn drop(&mut self) {
        println!("drop {}", self.0);
    }
}

fn main() {
    let positive = NonZeroI8::new(7).unwrap();
    assert_eq!((-positive).get(), -7);
    assert_eq!(Neg::neg(&positive).get(), -7);
    assert_eq!(WRAPPED.get(), i8::MIN);

    let contiguous = VecDeque::from([1, 2, 3]);
    let mut wrapped = VecDeque::with_capacity(4);
    wrapped.push_front(1);
    wrapped.push_back(2);
    wrapped.push_back(3);
    assert!(!wrapped.as_slices().1.is_empty());
    assert_eq!(contiguous, wrapped);
    wrapped[1] = 8;
    assert_ne!(contiguous, wrapped);
    wrapped.pop_back();
    assert_ne!(contiguous, wrapped);

    let mut storage = Box::new(MaybeUninit::<Payload>::uninit());
    let address = storage.as_mut_ptr();
    let payload = Box::write(storage, Payload(37));
    assert_eq!(ptr::from_ref(&*payload), address.cast_const());
    assert_eq!(payload.0, 37);
    drop(payload);
    let unit = Box::write(Box::new(MaybeUninit::<()>::uninit()), ());
    assert_eq!(std::mem::size_of_val(&*unit), 0);

    assert_eq!(DANGLING.addr(), align_of::<u32>());
    assert_eq!(NUMERIC.addr(), 4096);
    let mut words = [11u32, 29];
    let base = words.as_mut_ptr();
    let exposed = base as usize;
    assert_eq!(unsafe { *ptr::with_exposed_provenance::<u32>(exposed) }, 11);
    let pointer = AtomicPtr::new(ptr::with_exposed_provenance_mut::<u32>(exposed));
    let byte_step = size_of::<u32>();
    assert_eq!(pointer.fetch_byte_add(byte_step, Ordering::Release), base);
    assert_eq!(unsafe { *pointer.load(Ordering::Acquire) }, 29);
    assert_eq!(
        pointer.fetch_byte_sub(byte_step, Ordering::AcqRel),
        base.wrapping_add(1)
    );
    assert_eq!(pointer.fetch_or(1, Ordering::SeqCst), base);
    assert_eq!(pointer.fetch_xor(2, Ordering::Acquire).addr(), exposed | 1);
    assert_eq!(pointer.fetch_and(!3, Ordering::Relaxed).addr(), exposed | 3);
    assert_eq!(unsafe { *pointer.into_inner() }, 11);
    println!("library comparisons passed");
}
