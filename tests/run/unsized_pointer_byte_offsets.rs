use std::ptr::NonNull;
static DATA: [u8; 5] = [11, 13, 17, 19, 23];
const SLICE: &[u8] = match &DATA { [_, rest @ ..] => rest };
const POINTER: *const [u8] = SLICE;
const NEXT: *const [u8] = unsafe { POINTER.byte_add(1) };
const LENGTH: usize = NEXT.len();
const BACK: *const [u8] = unsafe { NEXT.byte_sub(1) };
const READ: u8 = unsafe { *(BACK as *const u8) };
trait Number { fn number(&self) -> u32; }
struct Value(u32);
impl Number for Value { fn number(&self) -> u32 { self.0 } }
static VALUE: Value = Value(42);
const OBJECT: *const dyn Number = &VALUE;
const OBJECT_BACK: *const dyn Number = unsafe { OBJECT.byte_add(1).byte_sub(1) };
fn shift<T: ?Sized>(pointer: *const T) -> *const T {
    // SAFETY: zero displacement requires no additional allocation bounds.
    unsafe { pointer.byte_add(0).byte_sub(0) }
}
fn main() {
    assert_eq!(LENGTH, 4);
    assert_eq!(READ, 13);
    assert_eq!(BACK.len(), 4);
    assert!(std::ptr::eq(BACK, POINTER));
    assert_eq!(unsafe { (&*OBJECT_BACK).number() }, 42);
    let mut data = [3u32, 5, 7];
    let pointer = &mut data[..] as *mut [u32];
    // SAFETY: four bytes remain within the three-word allocation; restoring
    // the pointer restores a valid slice with its original length.
    unsafe {
        let next = pointer.byte_add(4);
        assert_eq!(next.len(), 3);
        *(next as *mut u32) = 29;
        let back = next.byte_sub(4);
        assert_eq!((&*back)[1], 29);
        let wrapped = pointer.wrapping_byte_add(usize::MAX).wrapping_byte_add(1);
        assert!(std::ptr::eq(wrapped, pointer));
        let wrapped = pointer.wrapping_byte_sub(usize::MAX).wrapping_byte_sub(1);
        assert!(std::ptr::eq(wrapped, pointer));
        let nonnull = NonNull::new_unchecked(pointer).byte_add(4).byte_sub(4);
        assert!(std::ptr::eq(nonnull.as_ptr(), pointer));
    }
    let text: *const str = "hello";
    let restored = text.wrapping_byte_add(1).wrapping_byte_sub(1);
    assert_eq!(unsafe { &*restored }, "hello");
    assert!(std::ptr::eq(shift(text), text));
    let object = &VALUE as *const dyn Number;
    let restored = object.wrapping_byte_add(1).wrapping_byte_sub(1);
    assert_eq!(unsafe { (&*restored).number() }, 42);
    println!("{} {} {}", LENGTH, READ, data[1]);
}
