use core::mem::MaybeUninit;
struct Pair { first: u32, second: u32 }
const PARTIAL: MaybeUninit<Pair> = {
    let mut slot = MaybeUninit::<Pair>::uninit();
    unsafe { (&raw mut (*slot.as_mut_ptr()).first).write(35); }
    slot
};
const fn fill(slot: &mut MaybeUninit<u32>, value: u32) { slot.write(value); }
const SCALAR: u32 = {
    let mut slot = MaybeUninit::uninit();
    fill(&mut slot, 42);
    unsafe { slot.assume_init() }
};
const ARRAY: [u32; 3] = {
    let mut slot = MaybeUninit::<[u32; 3]>::uninit();
    let pointer = slot.as_mut_ptr().cast::<u32>();
    unsafe {
        pointer.add(2).write(3);
        pointer.write(1);
        let alias = pointer.add(1);
        alias.write(2);
        assert!(pointer.add(2).read() == 3);
        slot.assume_init()
    }
};
struct Empty((), [u8; 0]);
const EMPTY: Empty = unsafe { MaybeUninit::<Empty>::uninit().assume_init() };
enum Unit { Value }
const UNIT: Unit = unsafe { MaybeUninit::<Unit>::uninit().assume_init() };
struct Reordered { byte:u8, wide:u64, small:u16 }
const REORDERED:Reordered={
    let mut slot=MaybeUninit::<Reordered>::uninit();
    let pointer=slot.as_mut_ptr();
    unsafe {
        (&raw mut (*pointer).small).write(7);
        (&raw mut (*pointer).byte).write(3);
        (&raw mut (*pointer).wide).write(32);
        slot.assume_init()
    }
};
fn main() {
    println!("{} {:?}", SCALAR, ARRAY);
    println!("{}", REORDERED.byte as u64+REORDERED.wide+REORDERED.small as u64);
    let mut slot = PARTIAL;
    unsafe {
        (&raw mut (*slot.as_mut_ptr()).second).write(7);
        let pair = slot.assume_init();
        println!("{}", pair.first + pair.second);
    }
    println!("{} {}", core::mem::size_of_val(&EMPTY), matches!(UNIT, Unit::Value));
}
