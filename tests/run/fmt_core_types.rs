//! `core`'s other `Debug` and `Display` impls: C strings, UTF-8
//! errors, `fmt::Error`, `Location`, `Alignment`, raw pointers' `{:?}`
//! of null, and `Arguments`.

use core::ffi::CStr;
use core::fmt;
use core::panic::Location;

fn utf8(bytes: &[u8]) {
    match core::str::from_utf8(bytes) {
        Ok(s) => println!("ok {:?}", s),
        Err(e) => println!("{} / {:?} / {}", e, e, e.valid_up_to()),
    }
}

fn main() {
    println!("{:?}", c"plain");
    println!("{:?}", c"tab\tquote\"apostrophe'back\\slash\x01\x7f caf\u{e9}");
    let bytes = [b'a', 0xff, b'b', 0xe2, 0x82, 0];
    let c = CStr::from_bytes_with_nul(&bytes).unwrap();
    println!("{:?}", c);
    utf8(b"fine");
    utf8(&[b'x', 0xff, b'y']);
    utf8(&[b'x', 0xe2, 0x82]);
    println!("{:?} {}", fmt::Error, fmt::Error);
    let here = Location::caller();
    println!("{} {:?}", here, here.column());
    println!("{:?}", [fmt::Alignment::Left, fmt::Alignment::Center]);
    let null: *const u8 = 0usize as *const u8;
    println!("{:?} {:p}", null, null);
    println!("{:?} {}", format_args!("{}-{:?}", 1, "a"), format_args!("{:>4}", 'x'));
}
