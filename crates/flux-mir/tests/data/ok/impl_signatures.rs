// The lifetimes of methods whose signatures name their block's
// associated types, and of references dereferenced where a reference is
// expected.

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
        It { bag: self, i: 0 }
    }
}

struct Grid<T> {
    cells: [T; 2],
}

struct Cells<'a, T> {
    grid: &'a Grid<T>,
    i: usize,
}

impl<'a, T: Copy> Iterator for Cells<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<&'a T> {
        if self.i < 2 {
            self.i += 1;
            Some(&self.grid.cells[self.i - 1])
        } else {
            None
        }
    }
}

impl<'a, T: Copy> IntoIterator for &'a Grid<T> {
    type Item = &'a T;
    type IntoIter = Cells<'a, T>;

    fn into_iter(self) -> <Self as IntoIterator>::IntoIter {
        Cells { grid: self, i: 0 }
    }
}

fn largest<'a>(b: &'a Bag) -> &'a i32 {
    let mut best = &b.items[0];
    for x in b {
        if *x > *best {
            best = x;
        }
    }
    best
}

fn iter_of<'x>(b: &'x Bag) -> It<'x> {
    b.into_iter()
}

fn pick<'a>(r: &'a &'a Bag) -> &'a Bag {
    r
}

fn through<'a, 'b>(r: &'a mut &'b mut Bag) -> &'a Bag {
    r
}

fn main() {
    let b = Bag { items: [3, 4] };
    let g = Grid { cells: [5u8, 6] };
    let mut t = 0;
    for c in &g {
        t += *c as i32;
    }
    let mut c = Bag { items: [1, 2] };
    let mut m = &mut c;
    let r = through(&mut m);
    let _ = (t, largest(&b), iter_of(&b).i, pick(&&b), r);
}
