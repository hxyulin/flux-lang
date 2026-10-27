#[link(name = "c")]
#[link(name = "m", kind = "dylib")]
unsafe extern "C" {
    #[link_name = "in6addr_loopback"]
    static LOOPBACK: [u8; 16];
    #[link_name = "in6addr_loopback"]
    static SECOND_LOOPBACK: [u8; 16];
    static UNUSED_DOES_NOT_EXIST: u8;
    #[link_name = "optind"]
    static mut INDEX: i32;
}
#[link(name = "iconv")]
unsafe extern "C" {
    fn iconv_open(to: *const u8, from: *const u8) -> *mut core::ffi::c_void;
    fn iconv_close(value: *mut core::ffi::c_void) -> i32;
}
const LOOPBACK_ADDRESS: *const [u8; 16] = &raw const LOOPBACK;
fn main() {
    unsafe {
        assert_eq!(LOOPBACK[15], 1);
        assert_eq!(SECOND_LOOPBACK[15], 1);
        assert_eq!(&LOOPBACK[..15], &[0; 15]);
        assert_eq!((*LOOPBACK_ADDRESS)[15], 1);
        let original = INDEX;
        INDEX = 42;
        let current = INDEX;
        assert_eq!(current, 42);
        INDEX = original;
        let conversion = iconv_open(c"UTF-8".as_ptr().cast(), c"UTF-8".as_ptr().cast());
        assert_ne!(conversion as usize, usize::MAX);
        assert_eq!(iconv_close(conversion), 0);
    }
    println!("foreign globals and linked iconv");
}
