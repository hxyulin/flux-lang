// Valid Rust that flux does not support yet.
fn c_abi(p: extern "C" fn(i32)) {}
fn unsafe_c_abi(p: unsafe extern "C" fn(*const i8) -> i32) {}

fn main() {}
