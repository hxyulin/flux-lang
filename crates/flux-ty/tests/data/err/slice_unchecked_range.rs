fn main() {
    let mut a = [1u8, 2, 3];
    unsafe { a.get_unchecked(0..2); }
    unsafe { a.get_unchecked_mut(..); }
}
