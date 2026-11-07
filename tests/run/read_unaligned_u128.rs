#[inline(never)]
fn read(bytes: &[u8; 17]) -> u128 {
    // SAFETY: seventeen initialized bytes allow sixteen readable bytes at offset one.
    unsafe { bytes.as_ptr().add(1).cast::<u128>().read_unaligned() }
}
fn main() {
    let bytes = std::hint::black_box([1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17]);
    println!("{:x}", read(&bytes));
}
