// Calls of trait methods: they stay calls of the trait's method, with the
// self type among the arguments, and are resolved to impls when
// monomorphized. Receivers are moved, borrowed or reborrowed as the
// method's `self` asks.

pub struct S {
    x: i32,
}

pub struct W<T>(T);

trait Tr {
    fn get(&self) -> i32;

    fn set(&mut self, x: i32);

    fn take(self) -> i32;

    fn make(x: i32) -> Self;

    // Provided methods call the required ones on `self` and `Self`.
    fn twice(&self) -> i32 {
        self.get() + self.get()
    }

    fn remake(self) -> Self
    where
        Self: Sized,
    {
        let n = self.take();
        Self::make(n)
    }
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

    fn make(x: i32) -> Self {
        S { x }
    }
}

impl<T: Tr> Tr for W<T> {
    fn get(&self) -> i32 {
        self.0.get()
    }

    fn set(&mut self, x: i32) {
        self.0.set(x);
    }

    fn take(self) -> i32 {
        self.0.take()
    }

    fn make(x: i32) -> Self {
        W(T::make(x))
    }
}

impl Tr for i32 {
    fn get(&self) -> i32 {
        *self
    }

    fn set(&mut self, x: i32) {
        *self = x;
    }

    fn take(self) -> i32 {
        self
    }

    fn make(x: i32) -> Self {
        x
    }
}

// Each receiver form on a concrete type.
fn receivers(mut s: S) -> i32 {
    let a = s.get();
    s.set(2);
    let r = &mut s;
    r.set(3);
    let b = r.get();
    a + b + s.take()
}

// The same through a bound: a `T` is moved by a `self` method.
fn generic<T: Tr>(mut t: T) -> i32 {
    t.set(1);
    let a = t.get() + t.twice();
    a + t.remake().take()
}

// Path forms.
fn paths<T: Tr>(t: &T) -> i32 {
    let s: S = Tr::make(1);
    let a = <S as Tr>::get(&s);
    let b = T::get(t);
    let c = Tr::get(t);
    let d = <i32 as Tr>::take(4);
    a + b + c + d + S::take(s)
}

// A copy type's `self` method copies the receiver.
fn copy_receiver() -> i32 {
    let n = 5;
    let a = n.take();
    a + n.take() + 6.get()
}

// Receivers inside nested generic structs.
fn nested(w: &mut W<W<S>>) -> i32 {
    w.set(7);
    w.0.set(8);
    w.get() + w.0.0.get()
}

// A blanket impl.
trait Doubled {
    fn doubled(&self) -> i32;
}

impl<T: Tr> Doubled for T {
    fn doubled(&self) -> i32 {
        self.get() * 2
    }
}

fn blanket(s: &S) -> i32 {
    s.doubled() + 3.doubled()
}

fn main() {
    let _ = receivers(S { x: 1 }) + generic(W(S { x: 2 })) + paths(&4);
    let _ = copy_receiver() + nested(&mut W(W(S { x: 3 }))) + blanket(&S { x: 4 });
}
