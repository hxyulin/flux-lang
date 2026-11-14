use std::ffi::{CStr, c_char, c_int};
unsafe extern "C" {
    fn snprintf(dst: *mut c_char, len: usize, format: *const c_char, ...) -> c_int;
}
type Format = unsafe extern "C" fn(*mut c_char, usize, *const c_char, ...) -> c_int;
fn main() {
    let function: Format = snprintf;
    let mut buffer = [0 as c_char; 64];
    // SAFETY: the buffer is writable and the arguments match the format.
    let length = unsafe { function(buffer.as_mut_ptr(), buffer.len(), c"%d %.1f".as_ptr(), 42i32, 2.5f64) };
    // SAFETY: snprintf wrote a terminated string within the buffer.
    let text = unsafe { CStr::from_ptr(buffer.as_ptr()) };
    println!("{} {}", length, text.to_str().unwrap());
}
