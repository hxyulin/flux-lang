//! Panics. flux builds programs with `panic=abort`, so a panic aborts.

unsafe extern "C" {
    fn abort() -> !;
}

/// Aborts the program. Constant evaluation reports it instead, as
/// rustc's "evaluation panicked: " and `message`.
#[lang = "panic"]
pub(crate) const fn panic<M>(message: M) -> ! {
    unsafe { abort() }
}
