//! Errors expose formatting and an optional underlying cause.

pub trait Error: crate::fmt::Debug + crate::fmt::Display {
    fn source(&self) -> crate::option::Option<&(dyn Error + 'static)> {
        crate::option::Option::None
    }
}
