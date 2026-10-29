//! Panic-state queries and lazily initialized native thread-local storage.

/// Flux panics abort the process without unwinding or running destructors.
/// Ordinary executing code therefore never observes an unwinding thread.
pub fn panicking() -> bool {
    false
}

mod local;
pub use local::{AccessError, LocalKey};

mod scoped;
pub use scoped::{scope, spawn, JoinHandle, Scope, ScopedJoinHandle};

/// A joined thread's result. Worker panics currently abort in Flux.
pub type Result<T> = core::result::Result<T, crate::boxed::Box<dyn core::any::Any + Send + 'static>>;
