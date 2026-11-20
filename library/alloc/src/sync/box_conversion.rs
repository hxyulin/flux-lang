use super::{Arc, allocate};
use crate::alloc::{Layout, dealloc};
use crate::boxed::Box;
use core::marker::PhantomData;
use core::ptr::{self, NonNull};

impl<T: ?Sized> From<Box<T>> for Arc<T> {
    fn from(value: Box<T>) -> Self {
        let layout = Layout::for_value(&*value);
        let raw = allocate(layout);
        let source = Box::into_raw(value);
        let data = ptr::__with_metadata_of(raw, source);
        // SAFETY: the source owns an initialized T with this layout. Copying
        // to a fresh allocation transfers ownership without dropping T, and
        // pointer metadata remains valid for the same payload.
        unsafe {
            let size = layout.size();
            if size != 0 {
                ptr::copy_nonoverlapping(source as *const u8, raw, size);
                dealloc(source as *mut u8, layout);
            }
            Arc {
                data: NonNull::new_unchecked(data),
                owner: PhantomData,
            }
        }
    }
}
