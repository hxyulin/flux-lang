//! `core::ptr` and `core::mem`: null pointers, raw pointers' methods
//! (`add`, `sub`, `offset`, `offset_from`, `read`, `write`, `cast`),
//! `ptr::copy`, `copy_nonoverlapping`, `swap`, `replace`, `write_bytes`,
//! `drop_in_place` (counted by a `Drop` impl), `ptr::eq`, `NonNull`
//! (with its niche), `MaybeUninit` (which has none), `mem::forget` and
//! `ManuallyDrop`, which drop nothing.

use core::mem::{self, ManuallyDrop, MaybeUninit};
use core::ptr::{self, NonNull};

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

static mut DROPS: i64 = 0;

struct Noisy(i64);

impl Drop for Noisy {
    fn drop(&mut self) {
        unsafe {
            DROPS += self.0;
        }
    }
}

fn drops() -> i64 {
    unsafe { DROPS }
}

fn sum(p: *const u32, len: usize) -> i64 {
    let mut total = 0;
    for i in 0..len {
        total += unsafe { *p.add(i) } as i64;
    }
    total
}

fn main() {
    let p: *const u32 = ptr::null();
    let m: *mut u8 = ptr::null_mut();
    show(p.is_null() as i64 + m.is_null() as i64);
    let mut arr = [1u32, 2, 3, 4];
    let q = arr.as_mut_ptr();
    unsafe {
        show(*q.add(2) as i64);
        q.add(1).write(20);
        show(q.add(1).read() as i64);
        ptr::swap(q, q.add(3));
        ptr::copy(q, q.add(1), 2);
        show(q.offset(3).offset_from(q) as i64);
        show(*q.add(3).sub(1) as i64);
        let old = ptr::replace(q.add(2), 99);
        show(old as i64);
    }
    for x in arr {
        show(x as i64);
    }
    show(sum(arr.as_ptr(), 4));

    let src = [7u16, 8, 9];
    let mut dst = [0u16; 5];
    unsafe {
        ptr::copy_nonoverlapping(src.as_ptr(), dst.as_mut_ptr().add(1), 3);
        dst.as_mut_ptr().add(4).write_bytes(0xff, 1);
    }
    for x in dst {
        show(x as i64);
    }

    let nn = NonNull::new(q).unwrap();
    show(unsafe { *nn.as_ref() } as i64);
    let bytes: NonNull<u8> = nn.cast();
    show(unsafe { bytes.read() } as i64);
    show(NonNull::new(ptr::null_mut::<u8>()).is_none() as i64);
    show(size_of::<Option<NonNull<u8>>>() as i64);
    show(size_of::<MaybeUninit<Option<&u8>>>() as i64);
    show(size_of::<Option<MaybeUninit<&u8>>>() as i64);
    show((NonNull::<u64>::dangling().as_ptr() as usize) as i64);
    show(ptr::eq(&arr[0], &arr[0]) as i64 + ptr::eq(&arr[0], &arr[1]) as i64);

    let mut slot = MaybeUninit::<u64>::uninit();
    slot.write(42);
    show(unsafe { slot.assume_init() } as i64);
    let mut slots: [MaybeUninit<Noisy>; 3] =
        [MaybeUninit::uninit(), MaybeUninit::uninit(), MaybeUninit::uninit()];
    for (i, s) in slots.iter_mut().enumerate() {
        s.write(Noisy(i as i64 + 1));
    }
    unsafe {
        slots[0].assume_init_drop();
        show(slots[2].assume_init_ref().0);
        let p = slots[1].as_mut_ptr();
        ptr::drop_in_place(p);
    }
    show(drops());
    {
        let a = Noisy(100);
        let b = ManuallyDrop::new(Noisy(1000));
        show(b.0);
        mem::forget(a);
    }
    show(drops());
    let z: MaybeUninit<[u32; 2]> = MaybeUninit::zeroed();
    let z = unsafe { z.assume_init() };
    show(z[0] as i64 + z[1] as i64);
}
