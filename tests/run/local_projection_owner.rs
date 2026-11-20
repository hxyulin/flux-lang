use core::marker::PhantomData;
use core::ops::Deref;
use core::ptr::NonNull;

trait Family<'a> {
    type Output: ?Sized;
}

struct Wrapped<T: ?Sized>(PhantomData<T>);
struct Stack<'a, C>(C, PhantomData<&'a ()>);

impl<'a, C: Family<'a>> Deref for Stack<'a, C> {
    type Target = Wrapped<C::Output>;

    fn deref(&self) -> &Self::Target {
        let pointer: NonNull<Wrapped<C::Output>> = NonNull::from(self).cast();
        // SAFETY: Wrapped contains only PhantomData and has size zero and alignment one.
        unsafe { pointer.as_ref() }
    }
}

struct Concrete;
impl<'a> Family<'a> for Concrete {
    type Output = &'a u32;
}

fn main() {
    let stack = Stack(Concrete, PhantomData);
    let _: &Wrapped<&u32> = &*stack;
    assert_eq!(core::mem::size_of_val(&*stack), 0);
}
