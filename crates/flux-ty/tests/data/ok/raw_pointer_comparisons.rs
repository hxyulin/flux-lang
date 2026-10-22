fn compare<T>(a: *const T, b: *mut T) -> bool {
    a == b || a != b || a < b || a <= b || a > b || a >= b
}
fn mutable<T>(a: *mut T, b: *mut T) -> bool { a == b || a < b }
fn inferred(a: *const u8) -> bool { a == (0 as *const _) }
fn reference_rhs(a: *const u8, b: &u8) -> bool { a == b || a < b }
fn main() {}
