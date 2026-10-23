use std::cell::UnsafeCell;
use std::mem::{MaybeUninit, align_of, size_of};
use std::ptr::NonNull;

#[repr(align(64))]
struct Aligned(u8);
enum Void {}

struct Count<'a>(&'a UnsafeCell<u32>);
impl Drop for Count<'_> {
    fn drop(&mut self) {
        // SAFETY: Single-threaded counter, with no references into its payload.
        unsafe {
            *self.0.get() += 1;
        }
    }
}

struct Global(UnsafeCell<u32>);
// SAFETY: This program accesses the static only sequentially on its main thread.
unsafe impl Sync for Global {}
static GLOBAL: Global = Global(UnsafeCell::new(2));

const INITIAL: UnsafeCell<u32> = UnsafeCell::new(11);
fn pointer(cell: &UnsafeCell<u32>) -> *mut u32 {
    cell.get()
}

fn mutate(cell: &UnsafeCell<u32>, alias: &UnsafeCell<u32>, n: u32) -> u32 {
    // SAFETY: Accesses are sequential and no reference to the payload is live.
    unsafe {
        *cell.get() += n;
        let first = *alias.get();
        *alias.get() += 1;
        first + *cell.get()
    }
}

struct Chunk<T> {
    data: Box<[UnsafeCell<MaybeUninit<T>>]>,
}
// SAFETY: This example transfers only owned chunks and never shares writes.
unsafe impl<T: Send> Send for Chunk<T> {}
// SAFETY: This example publishes initialized slots only and never writes again.
unsafe impl<T: Send + Sync> Sync for Chunk<T> {}
fn chunk<T>(len: usize) -> Chunk<T> {
    Chunk {
        data: std::iter::repeat_with(|| UnsafeCell::new(MaybeUninit::uninit()))
            .take(len)
            .collect(),
    }
}
fn send<T: Send>() {}
fn sync<T: Sync>() {}

fn main() {
    send::<UnsafeCell<u32>>();
    send::<Chunk<u32>>();
    sync::<Chunk<u32>>();
    println!("static {}", mutate(&GLOBAL.0, &GLOBAL.0, 8));
    let mut cell = INITIAL;
    println!("{}", mutate(&cell, &cell, 5));
    *cell.get_mut() += 3;
    println!("{}", cell.into_inner());
    let value = UnsafeCell::new(19);
    // SAFETY: Pointer from the function still points to this live local.
    println!("{}", unsafe { *pointer(&value) });
    let aligned = UnsafeCell::new(Aligned(7));
    println!(
        "aligned {} {}",
        aligned.get() as usize % 64,
        aligned.get() as usize == &aligned as *const UnsafeCell<Aligned> as usize
    );
    println!(
        "{} {} {}",
        size_of::<UnsafeCell<Aligned>>(),
        align_of::<UnsafeCell<Aligned>>(),
        aligned.into_inner().0
    );
    println!(
        "{} {}",
        size_of::<UnsafeCell<()>>(),
        align_of::<UnsafeCell<()>>()
    );
    let zst = UnsafeCell::new(());
    // SAFETY: Aligned non-null pointer to a live zero-sized payload.
    unsafe {
        *zst.get() = ();
    }
    println!(
        "{} {} {} {} {}",
        size_of::<Option<UnsafeCell<NonNull<u8>>>>(),
        size_of::<UnsafeCell<Option<NonNull<u8>>>>(),
        size_of::<Option<UnsafeCell<bool>>>(),
        size_of::<UnsafeCell<Void>>(),
        size_of::<Option<UnsafeCell<Void>>>()
    );
    for value in [
        Some(UnsafeCell::new(true)),
        None,
        Some(UnsafeCell::new(false)),
    ] {
        match value {
            Some(cell) => println!("some {}", cell.into_inner()),
            None => println!("none"),
        }
    }
    let empty = chunk::<()>(5);
    println!("zst chunk {}", empty.data.len());
    let count = UnsafeCell::new(0);
    {
        let value = UnsafeCell::new(Count(&count));
        drop(value);
        let value = UnsafeCell::new(Count(&count));
        drop(value.into_inner());
        let mut value = UnsafeCell::new(Count(&count));
        let _ = value.get_mut();
        let value = UnsafeCell::new(MaybeUninit::new(Count(&count)));
        // SAFETY: The payload was initialized by MaybeUninit::new above.
        drop(unsafe { value.into_inner().assume_init() });
    }
    // SAFETY: All mutations to this counter have completed.
    println!("drops {}", unsafe { *count.get() });
    let values = chunk::<u32>(3);
    for i in 0..3 {
        // SAFETY: Each slot is exclusively initialized before being read.
        unsafe {
            (*values.data[i].get()).write(i as u32 + 20);
        }
    }
    for i in 0..3 {
        // SAFETY: Every slot was initialized above and remains alive.
        println!("{}", unsafe { (*values.data[i].get()).assume_init_ref() });
    }
}
