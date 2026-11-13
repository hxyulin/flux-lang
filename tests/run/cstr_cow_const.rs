use std::borrow::Cow;
use std::ffi::{CStr, CString};
const PREFIX: &CStr = match CStr::from_bytes_until_nul(b"hello\0tail") { Ok(s) => s, Err(_) => panic!() };
const EXACT: &CStr = match CStr::from_bytes_with_nul(b"hello\0") { Ok(s) => s, Err(_) => panic!() };
const MISSING: bool = CStr::from_bytes_until_nul(b"hello").is_err();
const INTERIOR: bool = CStr::from_bytes_with_nul(b"h\0i\0").is_err();
fn main() {
    assert_eq!(PREFIX, EXACT);
    assert!(MISSING && INTERIOR);
    let borrowed: Cow<'_, CStr> = PREFIX.into();
    assert!(matches!(borrowed, Cow::Borrowed(_)));
    let owned: Cow<'_, CStr> = CString::new("hello").unwrap().into();
    assert!(matches!(owned, Cow::Owned(_)));
    let string = CString::from(owned);
    let reborrowed: Cow<'_, CStr> = (&string).into();
    assert!(matches!(reborrowed, Cow::Borrowed(_)));
    assert_eq!(CString::from(borrowed), string);
    println!("{}", reborrowed.to_str().unwrap());
}
