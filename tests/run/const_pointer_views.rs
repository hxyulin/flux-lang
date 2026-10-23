use std::cell::Cell;
use std::num::NonZeroU32;

const REF: &u8 = unsafe {
    // SAFETY: Cell<T> preserves T's layout; the copied reference targets 1.
    *(&Cell::new(&1u8) as *const Cell<&u8> as *const &u8)
};
const fn copy_ref<'a>(value: &'a u16) -> &'a u16 {
    // SAFETY: the Cell is live for this read and has the reference's layout.
    unsafe { *(&Cell::new(value) as *const Cell<&u16> as *const &u16) }
}
const FORWARDED: &u16 = copy_ref(&321);
const fn read_view<T, U: Copy>(value: &T) -> U {
    // SAFETY: all call sites below use initialized, sufficiently aligned
    // storage large enough for U, with a valid U representation.
    unsafe { *(value as *const T as *const U) }
}
const BYTES: [u8; 4] = read_view::<u32, [u8; 4]>(&0x04030201);
const WORDS: [u16; 4] = read_view::<u64, [u16; 4]>(&0x0004000300020001);
const TRUE: bool = read_view::<u8, bool>(&1);
const FALSE: bool = read_view::<u16, bool>(&0);
const CHAR: char = read_view::<u32, char>(&0x1f980);
const NONZERO: NonZeroU32 = read_view::<u32, NonZeroU32>(&29);

#[repr(C)]
struct Wrapper<T>(T);
const WRAPPED: &u32 = unsafe {
    // SAFETY: the only field is at offset zero and is initialized.
    *(&Wrapper(&987u32) as *const Wrapper<&u32> as *const &u32)
};

struct Reordered {
    small: u8,
    wide: u64,
}
const REORDERED: u64 = unsafe {
    // SAFETY: on the pinned Rust/Flux layout the aligned u64 is at offset zero.
    *(&Reordered { small: 1, wide: 99 } as *const Reordered as *const u64)
};
static NUMBER: u32 = 0x08070605;
const POINTER: *const u8 = &NUMBER as *const u32 as *const u8;
const FIRST: &u8 = unsafe {
    // SAFETY: NUMBER's first byte is initialized and aligned for u8.
    &*POINTER
};
struct Pointers(*const u8, *const u16);
// SAFETY: both pointers address immutable static NUMBER storage.
unsafe impl Sync for Pointers {}
static POINTERS: Pointers = Pointers(POINTER, &NUMBER as *const u32 as *const u16);
const WHOLE: *const u8 = &0x0c0b0a09u32 as *const u32 as *const u8;
const THIN: *const u8 = {
    let slice: &[u8] = &[17, 18, 19];
    slice as *const [u8] as *const u8
};
const PREFIX: &[u8] = {
    let slice: &[u32] = &[0x04030201, 0x08070605];
    // SAFETY: wide casts retain two elements of metadata, covering the
    // first two initialized bytes. u8 has no alignment/validity restriction.
    unsafe { &*(slice as *const [u32] as *const [u8]) }
};
const SLICE_COPY: &[u16] = {
    let slice: &[u16] = &[21, 22, 23];
    // SAFETY: this copies the live Cell's initialized fat reference.
    unsafe { *(&Cell::new(slice) as *const Cell<&[u16]> as *const &[u16]) }
};
const UNIT: &() = unsafe {
    // SAFETY: this non-null, aligned static address suffices for a ZST.
    &*(&NUMBER as *const u32 as *const ())
};
const ZST_BOX: &Box<()> = unsafe {
    // SAFETY: a ZST Box needs a non-null aligned dangling pointer, no allocation.
    &*(&(1usize as *const ()) as *const *const () as *const Box<()>)
};
const TRANSIENT: u32 = {
    let value = 91;
    let r = &value;
    let again = &*r;
    *again
};

const fn promoted_arithmetic() -> *const u8 {
    &(1u8 + 1)
}
// SAFETY: the constant-only temporary is promoted, not a dead call-frame local.
const PURE: u8 = unsafe { *promoted_arithmetic() };
// SAFETY: usize and thin pointers have identical width/alignment. The pointer
// has no allocation provenance and is inspected only as initialized bits.
const ABSOLUTE: usize = unsafe { *(&(37usize as *const u8) as *const *const u8 as *const usize) };

fn main() {
    println!("{} {}", PURE, ABSOLUTE);
    println!("{} {} {} {}", REF, FORWARDED, WRAPPED, REORDERED);
    println!("{}", std::mem::size_of_val(&**ZST_BOX));
    println!("{:?} {:?}", BYTES, WORDS);
    println!("{} {} {} {}", TRUE, FALSE, CHAR, NONZERO.get());
    println!("{} {}", FIRST, FIRST as *const u8 == POINTER);
    for i in 0..4 {
        // SAFETY: both pointers retain their original four-byte allocations.
        println!("{} {}", unsafe { *POINTER.add(i) }, unsafe {
            *WHOLE.add(i)
        });
    }
    // SAFETY: these pointers address initialized NUMBER bytes and are aligned.
    println!("{} {}", unsafe { *POINTERS.0 }, unsafe { *POINTERS.1 });
    println!("{}", POINTERS.0 == POINTER);
    // SAFETY: THIN retains the original three-byte slice allocation.
    println!("{} {}", unsafe { *THIN }, unsafe { *THIN.add(2) });
    println!("{} {} {}", PREFIX.len(), PREFIX[0], PREFIX[1]);
    println!("{} {} {}", SLICE_COPY.len(), SLICE_COPY[0], SLICE_COPY[2]);
    println!(
        "{} {} {}",
        std::mem::size_of_val(UNIT),
        UNIT as *const () as *const u32 == &NUMBER as *const u32,
        TRANSIENT
    );
}
