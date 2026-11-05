//! Errors expose formatting and an optional underlying cause.

pub trait Error: crate::fmt::Debug + crate::fmt::Display {
    #[deprecated(note = "use the Display impl or to_string()")]
    fn description(&self) -> &str { "description() is deprecated; use Display" }

    #[deprecated(note = "replaced by Error::source, which can support downcasting")]
    fn cause(&self) -> crate::option::Option<&dyn Error> {
        self.source()
    }

    fn source(&self) -> crate::option::Option<&(dyn Error + 'static)> {
        crate::option::Option::None
    }
}
