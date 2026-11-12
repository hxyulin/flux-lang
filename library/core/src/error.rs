//! Errors expose formatting and an optional underlying cause.

pub trait Error: crate::fmt::Debug + crate::fmt::Display {
    #[deprecated(note = "use the Display impl or to_string()")]
    fn description(&self) -> &str { "description() is deprecated; use Display" }

    #[deprecated(note = "replaced by Error::source, which can support downcasting")]
    fn cause(&self) -> crate::option::Option<&dyn Error> {
        self.source()
    }

    #[doc(hidden)]
    fn type_id(&self, _: private::Internal) -> crate::any::TypeId where Self: 'static {
        crate::any::TypeId::of::<Self>()
    }

    fn source(&self) -> crate::option::Option<&(dyn Error + 'static)> {
        crate::option::Option::None
    }
}

mod private { pub struct Internal; }
impl dyn Error + 'static {
    pub fn is<T: Error + 'static>(&self) -> bool {
        self.type_id(private::Internal) == crate::any::TypeId::of::<T>()
    }
    pub fn downcast_ref<T: Error + 'static>(&self) -> Option<&T> {
        if self.is::<T>() {
            // SAFETY: the sealed dynamic identity proves that the data is T.
            Some(unsafe { &*(self as *const dyn Error as *const T) })
        } else { None }
    }
    pub fn downcast_mut<T: Error + 'static>(&mut self) -> Option<&mut T> {
        if self.is::<T>() {
            // SAFETY: identity proves T; the receiver guarantees exclusive access.
            Some(unsafe { &mut *(self as *mut dyn Error as *mut T) })
        } else { None }
    }
}
