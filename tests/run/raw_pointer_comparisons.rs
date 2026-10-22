// Comparisons are safe; none of these pointers are dereferenced. Integer
// addresses exercise the sign boundary without doing pointer arithmetic.
fn compare<T>(a: *const T, b: *const T) {
    println!("{} {} {} {} {} {}", a == b, a != b, a < b, a <= b, a > b, a >= b);
}
fn mixed<T>(a: *const T, b: *mut T) {
    println!("{} {} {} {} {} {}", a == b, a != b, a < b, a <= b, a > b, a >= b);
}
fn mutable<T>(a: *mut T, b: *mut T) {
    println!("{} {} {} {} {} {}", a == b, a != b, a < b, a <= b, a > b, a >= b);
}
fn main() {
    let values = [10u32, 20, 30, 40];
    let first = &values[0] as *const u32;
    let last = &values[3] as *const u32;
    // SAFETY: all offsets stay within this array or exactly one past it.
    let end = unsafe { first.add(4) };
    compare(first, first);
    compare(first, last);
    compare(last, first);
    compare(last, end);
    compare(end, end);
    compare(0 as *const u32, first);
    compare(0 as *const u8, 0 as *const u8);
    compare(0x7fffffffffffffffusize as *const u8, 0x8000000000000000usize as *const u8);
    compare(0xffffffffffffffffusize as *const u8, 0 as *const u8);
    mixed(first, first as *mut u32);
    mixed(first, last as *mut u32);
    mutable(first as *mut u32, last as *mut u32);
    mutable(last as *mut u32, first as *mut u32);
}
