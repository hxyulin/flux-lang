fn incompatible(a: *const u8, b: *const u16, m: *mut u8) {
    let _ = a == b;
    let _ = a < b;
    let _ = m == a;
    let _ = m < a;
    let _ = a == 0usize;
    let _ = a < (0 as *mut u16);
    let _ = a + a;
}
fn wide(a: *const [u8], b: *const [u8]) { let _ = a == b; let _ = a < b; }
fn generic_wide<T: ?Sized>(a: *const T, b: *const T) { let _ = a == b; }
const fn equality(a: *const u8, b: *const u8) -> bool { a == b }
const fn ordering(a: *const u8, b: *const u8) -> bool { a < b }
const INLINE: bool = const { (0 as *const u8) == (0 as *const u8) };
fn main() {}
