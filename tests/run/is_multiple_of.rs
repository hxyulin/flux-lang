const ZERO: bool = 0usize.is_multiple_of(0);
const NONZERO: bool = 1usize.is_multiple_of(0);
const MULTIPLE: bool = 128u8.is_multiple_of(64);

macro_rules! check {
    ($ty:ty) => {{
        for a in [0, 1, 2, 63, 64, 65, <$ty>::MAX - 1, <$ty>::MAX] {
            for b in [0, 1, 2, 3, 64, <$ty>::MAX] {
                let a: $ty = core::hint::black_box(a);
                let b: $ty = core::hint::black_box(b);
                let result = a.is_multiple_of(b);
                assert_eq!(result, if b == 0 { a == 0 } else { a % b == 0 });
                println!("{a} {b}: {result}");
            }
        }
    }};
}
fn main() {
    println!("{ZERO} {NONZERO} {MULTIPLE}");
    check!(u8);
    check!(u16);
    check!(u32);
    check!(u64);
    check!(u128);
    check!(usize);
}
