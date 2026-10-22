//! Hints to the compiler.

/// Returns `dummy`, which the optimizer must assume is used in any way.
pub const fn black_box<T>(dummy: T) -> T {
    dummy
}

/// Marks a path that cannot be reached by a valid execution.
///
/// # Safety
/// Reaching this function is undefined behavior.
#[rustc_intrinsic]
pub const unsafe fn unreachable_unchecked() -> ! {
    loop {}
}
