//! Diagnostic type descriptions. Other runtime type-reflection APIs are absent.

/// Returns a best-effort description of `T` with static storage.
///
/// Spelling is implementation-specific and lifetimes are omitted. The result
/// is not a unique identifier and must not be used to decide type identity.
/// Flux currently uses its diagnostic type printer, which may omit paths.
/// This function is not const, matching the stable Rust API.
pub fn type_name<T: ?Sized>() -> &'static str {
    crate::intrinsics::type_name::<T>()
}
