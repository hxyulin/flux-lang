//! Types for calling C: its integer types on flux's targets (64-bit
//! Apple arm64 and Linux), and `CStr`.

/// A borrowed C string, NUL-terminated: what `c"..."` gives, as
/// `&'static CStr`. The compiler knows its layout; only `as_ptr` is
/// supported yet.
#[lang = "CStr"]
pub struct CStr {
    _unsized: (),
}

pub type c_char = i8;
pub type c_schar = i8;
pub type c_uchar = u8;
pub type c_short = i16;
pub type c_ushort = u16;
pub type c_int = i32;
pub type c_uint = u32;
pub type c_long = i64;
pub type c_ulong = u64;
pub type c_longlong = i64;
pub type c_ulonglong = u64;
