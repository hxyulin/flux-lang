//! Conversions between types.

/// The identity function: returns its argument unchanged.
pub const fn identity<T>(x: T) -> T {
    x
}
