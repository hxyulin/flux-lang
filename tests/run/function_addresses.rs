fn value<T: Copy>(x: T) -> T {
    x
}
fn static_input(_: &'static u32) -> u32 {
    7
}
fn borrowed(value: &u32) -> &u32 {
    value
}
unsafe fn dangerous(value: u32) -> u32 {
    value + 2
}
extern "C" fn foreign(value: u32) -> u32 {
    value + 3
}
#[target_feature(enable = "neon")]
fn feature(value: u32) -> u32 {
    value + 4
}
fn generic_address<T: Copy>() -> usize {
    value::<T> as usize
}
macro_rules! alternative {
    ($body:expr) => {{
        #[cfg(all())] $body
        #[cfg(any())] false
    }};
}
macro_rules! outer {
    ($body:expr) => {
        alternative!($body)
    };
}
fn main() {
    let direct = value::<u64> as usize;
    let pointer: fn(u64) -> u64 = value::<u64>;
    assert_eq!(direct, pointer as usize);
    assert_eq!(generic_address::<u64>(), direct);
    assert!(generic_address::<u32>() != 0);
    let roundtrip: fn(u32) -> u32 = unsafe { core::mem::transmute(generic_address::<u32>()) };
    assert_eq!(roundtrip(41), 41);
    assert_eq!(value::<u64> as u128, direct as u128);
    assert_eq!(value::<u64> as u8, direct as u8);
    assert_eq!(value::<u64> as i128, direct as i128);
    assert!(static_input as usize != 0);
    assert!(borrowed as usize != 0);
    let call: unsafe fn(u32) -> u32 = dangerous;
    assert_eq!(call as usize, dangerous as usize);
    assert_eq!(unsafe { call(10) }, 12);
    let ccall: extern "C" fn(u32) -> u32 = foreign;
    assert_eq!(ccall as usize, foreign as usize);
    assert_eq!(ccall(10), 13);
    let c_roundtrip: extern "C" fn(u32) -> u32 = unsafe { core::mem::transmute(foreign as usize) };
    assert_eq!(c_roundtrip(40), 43);
    let borrowed_roundtrip: fn(&u32) -> &u32 = unsafe { core::mem::transmute(borrowed as usize) };
    let input = 67;
    assert_eq!(*borrowed_roundtrip(&input), 67);
    assert!(feature as usize != 0);
    assert!(outer!({ true }));
    let number = 3;
    let _: usize = ({
        let _ = number;
        value::<u32>
    }) as usize;
    println!(
        "function addresses {} {} {}",
        pointer(11),
        unsafe { call(11) },
        ccall(11)
    );
}
