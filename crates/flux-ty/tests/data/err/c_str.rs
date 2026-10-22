// Comparisons of `CStr` that rustc rejects: with a `str`, and a
// reference with a value (E0277), while `CStr` with `&CStr` is one of
// its impls.

use core::ffi::CStr;

fn with_str(a: &CStr, b: &str) -> bool {
    *a == *b
}

fn reference_with_value(a: &CStr, b: &CStr) -> bool {
    a == *b
}

fn reference_with_str(a: &CStr) -> bool {
    a == "x"
}

fn value_with_reference(a: &CStr, b: &CStr) -> bool {
    *a == b
}

fn main() {}
