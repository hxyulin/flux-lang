use core::ops::Deref;

struct S {
    n: i32,
}

struct Wrapper<T> {
    inner: T,
}

impl<T> Deref for Wrapper<T> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.inner
    }
}

fn takes(_: &u8) {}

fn main() {
    let s = S { n: 1 };
    let _a = *s;
    let w = Wrapper { inner: S { n: 2 } };
    let _b = w.missing;
    w.nothing();
    let _c: &u8 = &w;
    takes(&w);
    let _d: i32 = *w;
    let _e = *1u8;
}
