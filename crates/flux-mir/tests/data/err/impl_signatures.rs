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

    fn into_iter(self) -> Self::IntoIter {
        let b = Bag { items: [1, 2] };
        It { bag: &b, i: 0 }
    }
}

fn too_long<'x>(b: &'x Bag) -> It<'static> {
    b.into_iter()
}

fn escape(f: &Bag) -> &'static Bag {
    let r = &f;
    r
}

fn write_while_shared() {
    let mut f = Bag { items: [1, 2] };
    let mut m = &mut f;
    let mm = &mut m;
    let x: &Bag = mm;
    mm.items[0] = 3;
    let _ = x;
}

fn main() {}
