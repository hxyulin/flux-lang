//! Where panics happen.

use crate::fmt::{self, Display, Formatter};

/// Types whose owned values may cross an unwinding boundary.
#[lang = "unwind_safe"]
pub trait UnwindSafe {}

/// Types whose shared references may cross an unwinding boundary.
#[lang = "ref_unwind_safe"]
pub trait RefUnwindSafe {}

/// An explicit assertion that a value may cross an unwinding boundary.
#[derive(Clone, Copy, Debug, Default)]
pub struct AssertUnwindSafe<T>(pub T);

impl<T> UnwindSafe for AssertUnwindSafe<T> {}
impl<T> RefUnwindSafe for AssertUnwindSafe<T> {}

impl<T> crate::ops::Deref for AssertUnwindSafe<T> {
    type Target = T;
    fn deref(&self) -> &T { &self.0 }
}

impl<T> crate::ops::DerefMut for AssertUnwindSafe<T> {
    fn deref_mut(&mut self) -> &mut T { &mut self.0 }
}

impl<R, F: crate::ops::FnOnce() -> R> crate::ops::FnOnce<()> for AssertUnwindSafe<F> {
    type Output = R;
    fn call_once(self, _: ()) -> R { (self.0)() }
}

impl<F: crate::future::Future> crate::future::Future for AssertUnwindSafe<F> {
    type Output = F::Output;

    fn poll(
        self: crate::pin::Pin<&mut Self>,
        context: &mut crate::task::Context<'_>,
    ) -> crate::task::Poll<F::Output> {
        // SAFETY: the wrapper's pinned field stays in place for polling and
        // destruction; this projection never moves the future.
        unsafe { self.map_unchecked_mut(|wrapper| &mut wrapper.0) }.poll(context)
    }
}

/// A place in the source: a file, a line and a column, each counted
/// from one, as `#[track_caller]` functions get their caller's.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
#[lang = "Location"]
pub struct Location<'a> {
    file: &'a str,
    line: u32,
    col: u32,
}

impl<'a> Location<'a> {
    /// Where the function calling this was called, through
    /// `#[track_caller]` functions; where this is called otherwise.
    #[track_caller]
    pub fn caller() -> &'static Location<'static> {
        crate::intrinsics::caller_location()
    }

    /// The file's path, as the compiler was given it.
    pub const fn file(&self) -> &'a str {
        self.file
    }

    pub const fn line(&self) -> u32 {
        self.line
    }

    pub const fn column(&self) -> u32 {
        self.col
    }
}

/// `file:line:column`
impl Display for Location<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.file)?;
        f.write_str(":")?;
        Display::fmt(&self.line, f)?;
        f.write_str(":")?;
        Display::fmt(&self.col, f)
    }
}
