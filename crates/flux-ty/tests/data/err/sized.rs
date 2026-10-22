// `?Sized` parameters and implicit `Sized` bounds, as rustc reports them.

trait Tr {
    fn f(&self) -> i32;
}

impl Tr for str {
    fn f(&self) -> i32 {
        1
    }
}

impl<T: Tr> Tr for &T {
    fn f(&self) -> i32 {
        (**self).f()
    }
}

fn takes<T: Tr>(x: T) -> i32 {
    x.f()
}

fn by_value<T: ?Sized>(x: T) {}

fn sized<T>(x: &T) {}

fn pass<T: ?Sized>(x: &T) {
    sized(x);
}

struct S<T>(T);

fn in_struct<T: ?Sized>(x: &S<T>) {}

fn deref<T: ?Sized>(x: &T) {
    let y = *x;
}

fn main() {
    sized("a");
    let s: &str = "x";
    let t = *s;
    takes(s);
}
