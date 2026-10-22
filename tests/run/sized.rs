//! `?Sized` type parameters: generic functions, structs and `impl`
//! blocks over `str` and slices behind references, and `core`'s
//! comparisons of references to them.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

trait Size {
    fn size(&self) -> usize;
}

impl Size for str {
    fn size(&self) -> usize {
        self.len()
    }
}

impl<T> Size for [T] {
    fn size(&self) -> usize {
        self.len() * 10
    }
}

impl Size for i32 {
    fn size(&self) -> usize {
        4
    }
}

impl<T: ?Sized + Size> Size for &T {
    fn size(&self) -> usize {
        (**self).size() + 1
    }
}

fn size_of<T: ?Sized + Size>(x: &T) -> usize {
    x.size()
}

fn by_value<T: Size>(x: T) -> usize {
    x.size()
}

struct View<'a, T: ?Sized> {
    inner: &'a T,
    tag: u8,
}

impl<'a, T: ?Sized + Size> View<'a, T> {
    fn total(&self) -> usize {
        self.inner.size() + self.tag as usize
    }
}

fn same<T: ?Sized + PartialEq>(a: &T, b: &T) -> bool {
    a == b
}

fn less<T>(a: &T, b: &T) -> bool
where
    T: PartialOrd + ?Sized,
{
    a < b
}

fn first<T: ?Sized>(x: &T) -> *const T {
    x
}

fn main() {
    let s: &str = "hello";
    let a = [1, 2, 3];
    let sl: &[i32] = &a;
    let v = View { inner: s, tag: 3 };
    let w = View { inner: sl, tag: 1 };
    let n = 7;
    unsafe {
        printf(
            c"%zu %zu %zu %zu %zu %zu\n".as_ptr(),
            size_of(s),
            size_of(sl),
            size_of(&n),
            by_value(s),
            by_value(&s),
            by_value(sl),
        );
        printf(c"%zu %zu\n".as_ptr(), v.total(), w.total());
        printf(
            c"%d %d %d %d\n".as_ptr(),
            same("ab", "ab") as i32,
            same(s, "hellp") as i32,
            same(sl, &[1, 2, 3][..]) as i32,
            less("abc", "abd") as i32,
        );
        printf(c"%d %d\n".as_ptr(), (s == "hello") as i32, (&s == &"x") as i32);
        let p = first(s);
        printf(c"%d\n".as_ptr(), (p as *const u8 as usize == s.as_ptr() as usize) as i32);
    }
}
