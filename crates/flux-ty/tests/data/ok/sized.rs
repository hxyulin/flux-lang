// `?Sized` parameters: functions, structs and `impl` blocks over
// unsized types behind references.

trait Len {
    fn len_of(&self) -> usize;
}

impl Len for str {
    fn len_of(&self) -> usize {
        self.len()
    }
}

impl<T> Len for [T] {
    fn len_of(&self) -> usize {
        self.len()
    }
}

impl<T: ?Sized + Len> Len for &T {
    fn len_of(&self) -> usize {
        (**self).len_of()
    }
}

struct Ref<'a, T: ?Sized> {
    inner: &'a T,
}

impl<'a, T> Ref<'a, T>
where
    T: ?Sized + Len,
{
    fn len(&self) -> usize {
        self.inner.len_of()
    }
}

fn generic<T: ?Sized + Len>(x: &T) -> usize {
    let r = Ref { inner: x };
    r.len() + x.len_of()
}

fn sized<T: Len>(x: T) -> usize {
    x.len_of()
}

fn eq<T: ?Sized + PartialEq>(a: &T, b: &T) -> bool {
    a == b
}

fn main() {
    let s = "abc";
    let a = [1, 2];
    let _ = generic(s) + generic(&a[..]) + sized(s);
    let _ = eq("a", "b") && eq(&a[..], &a[..]);
}
