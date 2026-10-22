// A function returning an opaque type returns its hidden type; its
// callers hold the opaque type, which keeps the borrows it captures.

struct Bag {
    items: [u32; 4],
    len: usize,
}

impl Bag {
    fn evens(&self) -> impl Iterator<Item = u32> + '_ {
        self.items[..self.len].iter().copied().filter(|x| x % 2 == 0)
    }

    fn push(&mut self, x: u32) {
        self.items[self.len] = x;
        self.len += 1;
    }
}

fn adder(n: i32) -> impl Fn(i32) -> i32 {
    move |x| x + n
}

fn apply(f: impl Fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn owned() -> impl Iterator<Item = u32> {
    [1u32, 2, 3].into_iter().map(|x| x * 2)
}

fn main() {
    let mut bag = Bag {
        items: [0; 4],
        len: 0,
    };
    bag.push(2);
    let total: u32 = bag.evens().sum();
    bag.push(4);
    // Dropped at the end of the block, which may use what it borrows.
    let first = {
        let mut it = bag.evens();
        it.next()
    };
    let _ = (total, first);
    bag.push(6);
    let add = adder(1);
    let _ = apply(add, 2) + owned().count() as i32;
}
