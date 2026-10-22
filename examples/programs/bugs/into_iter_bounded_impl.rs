//! False rejection: `impl<'a, T: Copy> IntoIterator for &'a Bag<T>` is
//! reported as "method `into_iter` has an incompatible type for trait".
//! rustc accepts it and prints 7. Without the `T: Copy` bound on the
//! impl's parameter (here and on the `Iterator` impl), flux accepts it.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Bag<T> {
    items: [T; 2],
}

struct It<'a, T> {
    bag: &'a Bag<T>,
    i: usize,
}

impl<'a, T: Copy> Iterator for It<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.i < 2 {
            self.i += 1;
            Some(&self.bag.items[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a, T: Copy> IntoIterator for &'a Bag<T> {
    type Item = &'a T;
    type IntoIter = It<'a, T>;

    fn into_iter(self) -> It<'a, T> {
        It { bag: self, i: 0 }
    }
}

fn main() {
    let b = Bag { items: [3, 4] };
    let mut t = 0;
    for x in &b {
        t += *x;
    }
    unsafe {
        printf(c"%d\n".as_ptr(), t);
    }
}
