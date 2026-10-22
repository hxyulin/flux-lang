//! Repeated ownership transfers, including partial boxed-array patterns.
//! Also useful as a standalone executable under macOS `leaks --atExit`.
use core::cell::Cell;

struct Tracked<'a> {
    drops: &'a Cell<usize>,
    text: String,
}

impl Drop for Tracked<'_> {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + self.text.len());
    }
}

trait Value {
    fn len(&self) -> usize;
}

impl Value for Tracked<'_> {
    fn len(&self) -> usize {
        self.text.len()
    }
}

fn tracked(drops: &Cell<usize>) -> Tracked<'_> {
    Tracked {
        drops,
        text: String::from("owned"),
    }
}

fn exercise(drops: &Cell<usize>, take: bool) -> usize {
    let boxed = Box::new([
        tracked(drops),
        tracked(drops),
        tracked(drops),
        tracked(drops),
    ]);
    if take {
        let [first, middle @ .., _] = *boxed;
        drop(first);
        drop(middle);
    }

    let slice: Box<[Tracked<'_>]> = vec![tracked(drops), tracked(drops)].into_boxed_slice();
    let len = match &*slice {
        [first, rest @ ..] => first.text.len() + rest.len(),
        [] => 0,
    };
    drop(slice.into_vec());
    let object: Box<dyn Value + '_> = Box::new(tracked(drops));
    let len = len + object.len();

    let mut values = vec![
        tracked(drops),
        tracked(drops),
        tracked(drops),
        tracked(drops),
    ];
    {
        let mut drain = values.drain(1..3);
        drop(drain.next_back());
    }
    if let [first, .., last] = values.as_mut_slice() {
        first.text.push('!');
        last.text.push('?');
    }
    let mut iter = values.into_iter();
    drop(iter.next());

    let nested = Box::new(Box::new([String::from("a"), String::from("b")]));
    let [_, last] = **nested;
    len + last.len()
}

fn main() {
    let drops = Cell::new(0);
    let mut sum = 0;
    for i in 0..128 {
        sum += exercise(&drops, i % 2 == 0);
    }
    println!("{} {}", sum, drops.get());
}
