//! Hints to the compiler.

/// Returns `dummy`, which the optimizer must assume is used in any way.
pub const fn black_box<T>(dummy: T) -> T {
    dummy
}
