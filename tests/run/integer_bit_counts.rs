// Leading and trailing zeros, set bits and byte swaps of every integer
// width, at run time and in constants: zero, one, all bits set, the sign
// bit, and values whose set bits straddle the halves of a 128-bit word.

use std::hint::black_box;

const LZ_U8: u32 = 0u8.leading_zeros();
const TZ_U16: u32 = 0x8000u16.trailing_zeros();
const POP_I32: u32 = (-1i32).count_ones();
const SWAP_U64: u64 = 0x0102_0304_0506_0708u64.swap_bytes();
const LZ_U128: u32 = (1u128 << 64).leading_zeros();
const TZ_I128: u32 = i128::MIN.trailing_zeros();
const POP_U128: u32 = (u128::MAX >> 3).count_ones();
const SWAP_I128: i128 = 0x0102i128.swap_bytes();
const LOG_USIZE: u32 = 4096usize.ilog2();
const ONES_I8: u32 = (-2i8).leading_ones();
const TRAILING_ONES_U32: u32 = 0b0111u32.trailing_ones();
const ZEROS_I16: u32 = 0x00ffi16.count_zeros();
const NEXT_POWER: u64 = 1000u64.next_power_of_two();

macro_rules! report {
    ($($ty:ty),*) => {$(
        for x in [0 as $ty, 1, 2, 0x40, <$ty>::MAX, <$ty>::MIN, <$ty>::MAX / 3, <$ty>::MAX >> 1] {
            let x = black_box(x);
            println!(
                "{} {:?}: lz {} tz {} ones {} zeros {} lo {} to {} swap {:?} be {:?}",
                stringify!($ty),
                x,
                x.leading_zeros(),
                x.trailing_zeros(),
                x.count_ones(),
                x.count_zeros(),
                x.leading_ones(),
                x.trailing_ones(),
                x.swap_bytes(),
                x.to_be(),
            );
            if x > 0 as $ty {
                println!("  ilog2 {} checked {:?}", x.ilog2(), x.checked_ilog2());
            } else {
                println!("  checked {:?}", x.checked_ilog2());
            }
        }
    )*};
}

fn main() {
    report!(u8, i8, u16, i16, u32, i32, u64, i64, u128, i128, usize, isize);

    let straddle = black_box(0x0000_0000_0000_0001_8000_0000_0000_0000u128);
    println!(
        "{} {} {} {:#x}",
        straddle.leading_zeros(),
        straddle.trailing_zeros(),
        straddle.count_ones(),
        straddle.swap_bytes()
    );
    let low = black_box(0x10u128);
    println!("{} {}", low.leading_zeros(), low.trailing_zeros());
    let high = black_box(1i128 << 100);
    println!("{} {}", high.leading_zeros(), high.trailing_zeros());

    println!(
        "{} {} {} {:#x} {} {} {} {:#x} {} {} {} {} {}",
        LZ_U8, TZ_U16, POP_I32, SWAP_U64, LZ_U128, TZ_I128, POP_U128, SWAP_I128,
        LOG_USIZE, ONES_I8, TRAILING_ONES_U32, ZEROS_I16, NEXT_POWER
    );

    let mut checksum = 0u32;
    for i in 0..1000u32 {
        let x = black_box(i.wrapping_mul(2_654_435_761));
        checksum = checksum.wrapping_add(x.leading_zeros() * 3 + x.trailing_zeros() + x.count_ones());
        checksum ^= x.swap_bytes();
    }
    std::process::exit((checksum % 256) as i32);
}
