//! The `Fn`-family traits, which closures implement and calls of
//! values use. Stable Rust names them only as `Fn(A, B) -> C`, with
//! the parameters as a tuple and the result as `FnOnce`'s `Output`;
//! their methods may not be called by name, nor implemented by hand.

/// A call that may consume what is called: every closure has it.
#[lang = "fn_once"]
pub trait FnOnce<Args> {
    /// What the call returns.
    type Output;
    fn call_once(self, args: Args) -> Self::Output;
}

/// A call that may change what is called: closures that do not move
/// what they capture out.
#[lang = "fn_mut"]
pub trait FnMut<Args>: FnOnce<Args> {
    fn call_mut(&mut self, args: Args) -> Self::Output;
}

/// A call through a shared reference: closures that only read what
/// they capture.
#[lang = "fn"]
pub trait Fn<Args>: FnMut<Args> {
    fn call(&self, args: Args) -> Self::Output;
}

impl<A, F: Fn<A> + ?Sized> FnOnce<A> for &F {
    type Output = F::Output;
    fn call_once(self, args: A) -> F::Output { F::call(self, args) }
}

impl<A, F: Fn<A> + ?Sized> FnMut<A> for &F {
    fn call_mut(&mut self, args: A) -> F::Output { F::call(*self, args) }
}

impl<A, F: Fn<A> + ?Sized> Fn<A> for &F {
    fn call(&self, args: A) -> F::Output { F::call(*self, args) }
}

impl<A, F: FnMut<A> + ?Sized> FnOnce<A> for &mut F {
    type Output = F::Output;
    fn call_once(self, args: A) -> F::Output { F::call_mut(self, args) }
}

impl<A, F: FnMut<A> + ?Sized> FnMut<A> for &mut F {
    fn call_mut(&mut self, args: A) -> F::Output { F::call_mut(*self, args) }
}
