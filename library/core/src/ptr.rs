//! Raw pointers: `addr_of!` and `addr_of_mut!`, which make one to a
//! place without a reference to it, as `&raw const` and `&raw mut` do.

/// `&raw const $place`: a `*const` pointer to a place, made without a
/// reference to it.
macro_rules! addr_of {
    ($place:expr) => {
        &raw const $place
    };
}

/// `&raw mut $place`: a `*mut` pointer to a place, made without a
/// reference to it (so allowed of a `static mut`).
macro_rules! addr_of_mut {
    ($place:expr) => {
        &raw mut $place
    };
}

pub use addr_of;
pub use addr_of_mut;
