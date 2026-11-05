//! `Wake`, which turns an `Arc` into a `Waker`, and core's task types.
pub use core::task::*;

use crate::sync::Arc;
use core::marker::PhantomData;

/// A waker implemented on an `Arc`: `Waker::from(arc)` builds the raw
/// waker and its vtable.
pub trait Wake {
    /// Wakes this task, consuming one reference.
    fn wake(self: Arc<Self>);
    /// Wakes this task without consuming the reference.
    fn wake_by_ref(self: &Arc<Self>) {
        self.clone().wake();
    }
}

impl<W: Wake + Send + Sync + 'static> From<Arc<W>> for Waker {
    fn from(waker: Arc<W>) -> Waker {
        // SAFETY: `raw_waker` implements the RawWaker contract for an
        // `Arc<W>`, and `W: Send + Sync` lets any thread run the callbacks.
        unsafe { Waker::from_raw(raw_waker(waker)) }
    }
}

impl<W: Wake + Send + Sync + 'static> From<Arc<W>> for RawWaker {
    fn from(waker: Arc<W>) -> RawWaker {
        raw_waker(waker)
    }
}

/// One vtable per waker type.
struct Callbacks<W>(PhantomData<W>);

impl<W: Wake + Send + Sync + 'static> Callbacks<W> {
    const VTABLE: &'static RawWakerVTable = &RawWakerVTable::new(
        Self::clone_waker,
        Self::wake,
        Self::wake_by_ref,
        Self::drop_waker,
    );

    unsafe fn clone_waker(data: *const ()) -> RawWaker {
        // SAFETY: `data` came from `Arc::into_raw` of an `Arc<W>` that is
        // still owned by the waker being cloned.
        unsafe { Arc::increment_strong_count(data as *const W) };
        RawWaker::new(data, Self::VTABLE)
    }

    unsafe fn wake(data: *const ()) {
        // SAFETY: the waker owns one strong reference, which `wake` consumes.
        let waker = unsafe { Arc::from_raw(data as *const W) };
        <W as Wake>::wake(waker);
    }

    unsafe fn wake_by_ref(data: *const ()) {
        // SAFETY: borrow the waker's reference without taking ownership of it.
        let waker = core::mem::ManuallyDrop::new(unsafe { Arc::from_raw(data as *const W) });
        <W as Wake>::wake_by_ref(&*waker);
    }

    unsafe fn drop_waker(data: *const ()) {
        // SAFETY: the waker owns one strong reference, released here.
        unsafe { Arc::decrement_strong_count(data as *const W) };
    }
}

fn raw_waker<W: Wake + Send + Sync + 'static>(waker: Arc<W>) -> RawWaker {
    RawWaker::new(Arc::into_raw(waker) as *const (), Callbacks::<W>::VTABLE)
}
