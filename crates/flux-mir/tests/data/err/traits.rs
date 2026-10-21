// Moves and borrows through trait methods: a `self` method moves its
// receiver, a `&mut self` method borrows it mutably, and a `T` is never
// `Copy`.

struct S {
    x: i32,
}

trait Tr {
    fn get(&self) -> i32;

    fn set(&mut self, x: i32);

    fn take(self) -> i32;
}

impl Tr for S {
    fn get(&self) -> i32 {
        self.x
    }

    fn set(&mut self, x: i32) {
        self.x = x;
    }

    fn take(self) -> i32 {
        self.x
    }
}

// Use after a `self` method moved the receiver.
fn use_after_take(s: S) -> i32 {
    let a = s.take();
    a + s.get()
}

fn take_twice<T: Tr>(t: T) -> i32 {
    t.take() + t.take()
}

fn take_through_path(s: S) -> i32 {
    let a = Tr::take(s);
    a + <S as Tr>::get(&s)
}

fn take_in_loop(s: S) {
    loop {
        s.take();
    }
}

// A `self` method cannot move out of a reference.
fn take_behind_ref(r: &S) -> i32 {
    r.take()
}

fn take_behind_generic_ref<T: Tr>(r: &T) -> i32 {
    r.take()
}

fn take_field_behind_ref(r: &(S, i32)) -> i32 {
    r.0.take()
}

// A `&mut self` method needs a mutable place.
fn set_immutable(s: S) {
    s.set(1);
}

fn set_through_shared(r: &S) {
    r.set(1);
}

fn set_generic<T: Tr>(t: T) {
    t.set(1);
}

fn set_through_path(s: S) {
    Tr::set(&mut s, 1);
}

// A borrow held across a call that needs the receiver.
fn borrow_then_set(mut s: S) -> i32 {
    let r = &s;
    s.set(2);
    r.get()
}

fn borrow_then_take(s: S) -> i32 {
    let r = &s;
    let a = s.take();
    a + r.get()
}

// A provided method that takes `self` moves it.
trait Twice {
    fn once(self) -> i32;

    fn twice(self) -> i32
    where
        Self: Sized,
    {
        self.once() + self.once()
    }
}

fn main() {}
