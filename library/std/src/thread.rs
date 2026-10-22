//! Only the panic-state query is supported; no thread creation or handles.

/// Flux panics abort the process without unwinding or running destructors.
/// Ordinary executing code therefore never observes an unwinding thread.
pub fn panicking() -> bool {
    false
}
