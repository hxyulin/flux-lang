//! False rejection: an `IntoIterator` impl for a reference whose
//! `into_iter` returns `Self::IntoIter` (rather than spelling out
//! `It<'a>`) is reported as "`impl` item signature doesn't match `trait`
//! item signature" and "lifetime may not live long enough". rustc
//! accepts it and prints 7; writing `-> It<'a>` works in flux.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

struct Bag {
    items: [i32; 2],
}

struct It<'a> {
    bag: &'a Bag,
    i: usize,
}

impl<'a> Iterator for It<'a> {
    type Item = &'a i32;

    fn next(&mut self) -> Option<&'a i32> {
        if self.i < 2 {
            self.i += 1;
            Some(&self.bag.items[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a> IntoIterator for &'a Bag {
    type Item = &'a i32;
    type IntoIter = It<'a>;

    fn into_iter(self) -> Self::IntoIter {
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
