#[repr(C, align(64))]
struct Align([u8; 0]);
#[repr(C)]
struct Aligned { align: [Align; 0], bytes: [u8; 8] }
const fn bytes() -> &'static [u8; 8] {
    const DATA: Aligned = Aligned { align: [], bytes: [173; 8] };
    &DATA.bytes
}
static RESULT: &[u8; 8] = bytes();
const ELEMENT: &u8 = &bytes()[3];
const ARRAY: [u32; 3] = [17, 23, 31];
fn element(index: usize) -> &'static u32 { &ARRAY[index] }
fn plus(x: i32) -> i32 { x + 7 }
const CALLBACK: (fn(i32) -> i32, u32) = (plus, 2);
static CALLBACK_REF: &fn(i32) -> i32 = &CALLBACK.0;
fn callback() -> &'static fn(i32) -> i32 { &CALLBACK.0 }
fn main() {
    let value = bytes();
    assert_eq!(value.as_ptr() as usize % 64, 0);
    assert_eq!(RESULT.as_ptr() as usize % 64, 0);
    println!("{} {} {} {} {} {}", value[0], RESULT[7], *ELEMENT, element(1), (*callback())(3), (*CALLBACK_REF)(5));
}
