use core::{cell::UnsafeCell, mem::MaybeUninit};

struct Slot<T>(UnsafeCell<MaybeUninit<T>>);
unsafe impl<T: Send + Sync> Sync for Slot<T> {}
impl<T> Slot<T> {
    const fn new() -> Self { Self(UnsafeCell::new(MaybeUninit::uninit())) }
}
static SLOT: Slot<Vec<u32>> = Slot::new();
const EMPTY: [MaybeUninit<u32>; 4] = [MaybeUninit::uninit(); 4];
const BORROW: MaybeUninit<&u32> = MaybeUninit::uninit();
const OPTIONAL: Option<MaybeUninit<bool>> = Some(MaybeUninit::uninit());
const READY: u32 = unsafe { MaybeUninit::new(42).assume_init() };
enum Void {}
const VOID: MaybeUninit<Void> = MaybeUninit::uninit();
fn main() {
    let mut values = EMPTY;
    for (i, value) in values.iter_mut().enumerate() { value.write(i as u32 + READY); }
    let sum: u32 = values.into_iter().map(|value| unsafe { value.assume_init() }).sum();
    let mut reference = BORROW;
    reference.write(&READY);
    println!("{} {} {} {}", sum, unsafe { *reference.assume_init() }, OPTIONAL.is_some(), core::mem::size_of_val(&VOID));
    unsafe {
        SLOT.0.get().write(MaybeUninit::new(vec![3, 7]));
        let owned = SLOT.0.get().read().assume_init();
        println!("{}", owned.iter().copied().sum::<u32>());
    }
}
