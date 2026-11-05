// Guard markers whose `T: 'a` bounds follow from their fields, used to
// share a guard with a scoped thread.
use std::cell::UnsafeCell;
use std::marker::PhantomData;

pub struct Lock<T> {
    data: UnsafeCell<T>,
}

pub struct Guard<'a, T: ?Sized> {
    data: *mut T,
    marker: PhantomData<&'a mut T>,
}

unsafe impl<'a, T: ?Sized + Sync + 'a> Sync for Guard<'a, T> {}

impl<'a, T: ?Sized> Guard<'a, T> {
    fn get(&self) -> &T {
        unsafe { &*self.data }
    }
}

fn lock<T>(l: &Lock<T>) -> Guard<'_, T> {
    Guard { data: l.data.get(), marker: PhantomData }
}

fn share<T: Sync>(_: &T) -> bool {
    true
}

fn main() {
    let l = Lock { data: UnsafeCell::new(41u32) };
    let g = lock(&l);
    println!("{}", share(&g));
    std::process::exit(*g.get() as i32 + 1);
}
