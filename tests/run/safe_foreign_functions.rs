unsafe extern "C" {
    safe fn abs(value: i32) -> i32;
}
fn main() {
    assert_eq!(abs(-7), 7);
    let pointer: extern "C" fn(i32) -> i32 = abs;
    assert_eq!(pointer(-11), 11);
    println!("safe foreign calls and pointers passed");
}
