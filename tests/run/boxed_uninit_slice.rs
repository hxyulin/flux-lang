//! Deferred initialization never reads unwritten slots or drops their payloads.
use std::cell::Cell;
use std::mem::{MaybeUninit, align_of, size_of};

struct Owned<'a> {
    text: String,
    drops: &'a Cell<usize>,
}

impl Drop for Owned<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}

#[repr(align(64))]
struct Aligned(u64);

#[repr(align(64))]
struct AlignedZst;

fn numbers(len: usize) {
    let mut slots = Box::<[u64]>::new_uninit_slice(len);
    assert_eq!(slots.len(), len);
    assert_eq!(slots.as_ptr() as usize % align_of::<u64>(), 0);
    for i in 0..len {
        slots[i].write(i as u64 * 7 + 3);
    }
    let mut sum = 0;
    for slot in &*slots {
        // SAFETY: every slot was written above.
        sum += unsafe { slot.assume_init_ref() };
    }
    // Metadata and layout survive a raw ownership round trip and Vec conversion.
    let raw = Box::into_raw(slots);
    // SAFETY: raw came from this box, and ownership has not been reused.
    let back = unsafe { Box::from_raw(raw) };
    let values = back.into_vec();
    assert_eq!(values.len(), len);
    assert_eq!(values.capacity(), len);
    println!("numbers {len} {sum}");
}

fn owned(len: usize) {
    let drops = Cell::new(0);
    let mut slots = Box::<[Owned<'_>]>::new_uninit_slice(len);
    assert_eq!(drops.get(), 0);
    // Only initialize alternating slots: the others remain unread.
    for i in (0..len).step_by(2) {
        slots[i].write(Owned {
            text: String::from("owned"),
            drops: &drops,
        });
    }
    for i in (0..len).step_by(2) {
        // SAFETY: this slot was written and is read exactly once; the remaining
        // MaybeUninit wrapper will not drop the moved payload.
        let value = unsafe { slots[i].assume_init_read() };
        assert_eq!(value.text, "owned");
        drop(value);
    }
    let initialized = len.div_ceil(2);
    assert_eq!(drops.get(), initialized);
    drop(slots);
    assert_eq!(drops.get(), initialized);
    println!("owned {len} {}", drops.get());
}

fn aligned(len: usize) {
    let mut slots = Box::<[Aligned]>::new_uninit_slice(len);
    assert_eq!(slots.len(), len);
    assert_eq!(slots.as_ptr() as usize % 64, 0);
    let layout = std::alloc::Layout::for_value(&*slots);
    assert_eq!(layout.size(), len * size_of::<Aligned>());
    assert_eq!(layout.align(), 64);
    for i in 0..len {
        slots[i].write(Aligned(i as u64));
    }
    for i in 0..len {
        // SAFETY: all slots were initialized above.
        assert_eq!(unsafe { slots[i].assume_init_ref() }.0, i as u64);
    }
    drop(slots);
    println!("aligned {len}");
}

fn main() {
    for len in [0, 1, 511, 512, 513, 2048] {
        numbers(len);
        owned(len);
        aligned(len);
    }
    struct DropOnly<'a>(&'a Cell<usize>);
    impl Drop for DropOnly<'_> {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let drops = Cell::new(0);
    let mut partial = Box::<[DropOnly<'_>]>::new_uninit_slice(513);
    partial[256].write(DropOnly(&drops));
    drop(partial);
    assert_eq!(drops.get(), 0);
    // An uninhabited payload proves no T is constructed, even for nonempty boxes.
    enum Never {}
    let empty_values = Box::<[Never]>::new_uninit_slice(3);
    assert_eq!(empty_values.len(), 3);
    drop(empty_values);
    for len in [0, 1, 513] {
        let slots = Box::<[AlignedZst]>::new_uninit_slice(len);
        assert_eq!(size_of::<MaybeUninit<AlignedZst>>(), 0);
        assert_eq!(slots.len(), len);
        assert_eq!(slots.as_ptr() as usize % 64, 0);
        drop(slots);
        println!("zst {len}");
    }
    // No allocation backs this length beyond isize::MAX. Transfer ownership
    // without traversing usize::MAX empty elements in generic slice drop glue.
    let huge = Box::<[AlignedZst]>::new_uninit_slice(usize::MAX);
    assert_eq!(huge.len(), usize::MAX);
    assert_eq!(huge.as_ptr() as usize % 64, 0);
    let _ = Box::into_raw(huge);
    println!("huge zst {}", usize::MAX);
    // Leaking retains initialized contents and length, as the arena consumer needs.
    let mut slots = Box::<[u64]>::new_uninit_slice(1);
    slots[0].write(42);
    let leaked = Box::leak(slots);
    assert_eq!(leaked.len(), 1);
    // SAFETY: the only element was initialized before leaking.
    println!("leaked {}", unsafe { leaked[0].assume_init_ref() });
}
