use core::ffi::c_char;
unsafe extern "C" {
    #[link_name = "strlen"]
    fn length(pointer: *const c_char) -> usize;
    #[link_name = r"abs"]
    fn magnitude(value: i32) -> i32;
    #[cfg_attr(all(), link_name = "strlen")]
    fn configured(pointer: *const c_char) -> usize;
    #[link_name = "strlen"]
    #[link_name = "abort"]
    fn first(pointer: *const c_char) -> usize;
}
#[link_name = "strlen"]
fn ordinary(value: u32) -> u32 {
    value + 7
}
fn main() {
    let text = c"aé中";
    unsafe {
        assert_eq!(length(text.as_ptr()), 6);
        assert_eq!(magnitude(-37), 37);
        assert_eq!(configured(c"hello".as_ptr()), 5);
        assert_eq!(first(c"first".as_ptr()), 5);
        let callback: unsafe extern "C" fn(*const c_char) -> usize = length;
        assert_eq!(callback(c"pointer".as_ptr()), 7);
    }
    assert_eq!(ordinary(5), 12);
    println!("foreign link aliases 6 37 5 5 7 12");
}
