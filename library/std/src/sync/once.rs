//! One-time initialization with poisoning after an initializer unwinds.
use super::OnceLock;
use core::sync::atomic::{AtomicBool, Ordering};

pub struct Once {
    completed: OnceLock<()>,
    poisoned: AtomicBool,
}

pub struct OnceState {
    poisoned: bool,
}

impl OnceState {
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
}

impl Once {
    pub const fn new() -> Self {
        Self {
            completed: OnceLock::new(),
            poisoned: AtomicBool::new(false),
        }
    }
    pub fn is_completed(&self) -> bool {
        self.completed.get().is_some()
    }
    pub fn call_once<F: FnOnce()>(&self, f: F) {
        self.call_once_force(|state| {
            assert!(
                !state.is_poisoned(),
                "Once instance has previously been poisoned"
            );
            f();
        });
    }
    pub fn call_once_force<F: FnOnce(&OnceState)>(&self, f: F) {
        self.completed.get_or_init(|| {
            let state = OnceState {
                poisoned: self.poisoned.swap(true, Ordering::Acquire),
            };
            f(&state);
            self.poisoned.store(false, Ordering::Release);
        });
    }
}

/// A Once in its initial state.
#[deprecated(note = "the `Once::new()` function is now preferred")]
pub const ONCE_INIT: Once = Once::new();
