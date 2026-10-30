//! Hints to the compiler.

/// Returns `dummy`, which the optimizer must assume is used in any way.
pub const fn black_box<T>(dummy: T) -> T {
    dummy
}

/// Marks a short busy-wait iteration.
///
/// This implementation uses the no-op fallback. It does not yield to the
/// scheduler or provide synchronization; use atomics for communication.
pub fn spin_loop() {}

/// Marks a path that cannot be reached by a valid execution.
///
/// # Safety
/// Reaching this function is undefined behavior.
#[rustc_intrinsic]
pub const unsafe fn unreachable_unchecked() -> ! {
    loop {}
}

/// Selects a value without promising which condition is more likely.
pub fn select_unpredictable<T>(condition: bool, if_true: T, if_false: T) -> T {
    if condition { if_true } else { if_false }
}
