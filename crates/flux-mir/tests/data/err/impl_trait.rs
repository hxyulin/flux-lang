// Borrows that opaque types keep, and hidden types that borrow what
// they may not.

struct Bag {
    items: [u32; 4],
    len: usize,
}

impl Bag {
    fn evens(&self) -> impl Iterator<Item = u32> + '_ {
        self.items[..self.len].iter().copied().filter(|x| x % 2 == 0)
    }

    fn odds(&self) -> impl Iterator<Item = u32> {
        self.items[..self.len].iter().copied().filter(|x| x % 2 == 1)
    }

    fn push(&mut self, x: u32) {
        self.items[self.len] = x;
        self.len += 1;
    }
}

fn evens_of(v: &[u32]) -> impl Iterator<Item = u32> {
    v.iter().copied().filter(|x| x % 2 == 0)
}

fn uncaptured(v: &[u32]) -> impl Iterator<Item = u32> + use<> {
    v.iter().copied()
}

fn adder(n: i32) -> impl Fn(i32) -> i32 {
    |x| x + n
}

fn local() -> impl Iterator<Item = u32> {
    let a = [1u32, 2];
    a.iter().copied()
}

fn kept() {
    let mut bag = Bag {
        items: [0; 4],
        len: 0,
    };
    let it = bag.evens();
    bag.push(3);
    let _ = it.count();
    let o = bag.odds();
    bag.push(4);
    let _ = o.count();
    let mut a = [1u32, 2, 3];
    let e = evens_of(&a);
    a[0] = 4;
    let _ = e.count();
    let b = [1u32, 2];
    let f = evens_of(&b);
    drop(b);
    let _ = f.count();
}

fn moved() {
    let f = adder(1);
    let g = f;
    let _ = f(1) + g(2);
}

fn main() {
    kept();
    moved();
}
