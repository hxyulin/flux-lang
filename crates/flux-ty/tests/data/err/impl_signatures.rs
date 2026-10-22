struct Bag {
    items: [i32; 2],
}

struct It<'a> {
    bag: &'a Bag,
    i: usize,
}

impl<'a> Iterator for It<'a> {
    type Item = &'a i32;

    fn next(&mut self) -> Option<Self::Item> {
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

    fn into_iter(self) -> Self::Item {
        &self.items[0]
    }
}

trait Tr {
    type A;

    fn f(&self, a: Self::A) -> i32;
}

struct S;

impl Tr for S {
    type A = u8;

    fn f(&self, a: <Self as Tr>::A) -> u32 {
        a as u32
    }
}

struct Q;

impl Tr for Q {
    type A = u8;

    fn f(&self, a: Self::A) -> <Self as Tr>::A {
        a
    }
}

fn main() {}
