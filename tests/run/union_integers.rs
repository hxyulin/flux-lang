#[derive(Copy, Clone)]
#[repr(C)]
union Bits {
    word: u64,
    signed: i64,
    byte: u8,
    #[cfg(any())]
    word: Missing,
    #[cfg_attr(all(), cfg(any()))]
    ignored: [Missing; 4],
}

#[derive(Copy, Clone)]
union Native {
    half: u16,
    signed: i16,
    byte: u8,
}

fn make(value: u64) -> Bits { Bits { word: value } }
fn change(bits: &mut Bits, value: u64) { bits.word = value; }
fn read(bits: Bits) -> u64 { unsafe { bits.word } }
#[derive(Copy, Clone)]
union Wide { big: u128, signed: i128, small: u8 }
#[derive(Copy, Clone)]
struct Wrapped { before: u8, bits: Bits, after: u16 }
union Dropped { value: u32, other: i32 }
impl Drop for Dropped {
    fn drop(&mut self) { unsafe { println!("drop {}", self.value); } }
}
fn pass_wide(value: Wide) -> Wide { value }
fn main() {
    let mut bits = make(0x123456789abcdef0);
    println!("{} {}", core::mem::size_of::<Bits>(), core::mem::align_of::<Bits>());
    unsafe {
        println!("{} {} {}", bits.word, bits.byte, bits.signed);
        let a = &bits.word as *const u64 as usize;
        let b = &bits.byte as *const u8 as usize;
        println!("{}", a == b);
    }
    let copy = bits;
    let cloned = bits.clone();
    change(&mut bits, 99);
    println!("{} {} {}", read(bits), read(copy), read(cloned));
    bits.byte = 7;
    unsafe { println!("{}", bits.byte); }
    let late: Bits;
    late = Bits { word: 123 };
    println!("{}", read(late));
    let mut native = Native { signed: -2 };
    unsafe { println!("{} {}", native.half, native.byte); }
    unsafe { *&mut native.half = 1234; }
    unsafe { println!("{}", native.signed); }
    println!("{} {}", core::mem::size_of::<Native>(), core::mem::align_of::<Native>());
    let wide = pass_wide(Wide { big: 0x123456789abcdef0123456789abcdef0 });
    let wide_copy = wide.clone();
    unsafe { println!("{} {} {}", wide.big, wide_copy.signed, wide.small); }
    println!("{} {}", core::mem::size_of::<Wide>(), core::mem::align_of::<Wide>());
    let never_read = Bits { byte: 1 };
    let unused = move || unsafe { never_read.word };
    drop(unused); // Capturing must not eagerly read an uninitialized word.
    let closure_bits = make(87);
    let closure = move || unsafe { closure_bits.word };
    println!("{}", closure());
    let small = Bits { byte: 19 };
    let small_copy = small.clone();
    unsafe { println!("{} {}", small.byte, small_copy.byte); }
    let wrapped = Wrapped { before: 3, bits: make(55), after: 42 };
    let copied = wrapped.clone();
    println!("{} {} {}", copied.before, read(copied.bits), copied.after);
    let array = [make(1), make(2)];
    println!("{} {}", read(array[0]), read(array[1]));
    let dropped = Dropped { value: 77 };
    let moved = dropped;
    unsafe { println!("{}", moved.other); }
}
