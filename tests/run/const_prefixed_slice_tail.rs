#[repr(C, packed)]
struct Exception {
    first: u8,
    second: u8,
    rest: [u8],
}
static BYTES: &[u8] = &[7, 29, 43];
const HEADER: &Exception = unsafe {
    let slice = core::ptr::slice_from_raw_parts(BYTES.as_ptr(), 0);
    &*(slice as *const Exception)
};

const WITH_TAIL: &Exception = unsafe {
    let slice = core::ptr::slice_from_raw_parts(BYTES.as_ptr(), 1);
    &*(slice as *const Exception)
};

fn main() {
    assert_eq!(HEADER.first, 7);
    assert_eq!(HEADER.second, 29);
    assert!(HEADER.rest.is_empty());
    assert_eq!(&WITH_TAIL.rest, &[43]);
}
