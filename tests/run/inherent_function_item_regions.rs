struct Filter<'a>(&'a u8);

impl<'a> Filter<'a> {
    fn matches(value: u8) -> bool {
        value > 3
    }
}

impl Filter<'_> {
    fn rejects(value: u8) -> bool {
        value <= 3
    }
}

trait Framework {
    fn run(&self) -> bool;
}

impl<'a> Framework for Filter<'a> {
    fn run(&self) -> bool {
        let callback = Self::matches;
        let accepts = apply(callback, *self.0);
        assert_eq!(accepts, !apply(Self::rejects, *self.0));
        accepts
    }
}

fn apply(callback: impl Fn(u8) -> bool + 'static, value: u8) -> bool {
    callback(value)
}

struct Pair<'a, 'b>(&'a u8, &'b u8);

impl<'b, 'a> Pair<'a, 'b> {
    fn first(a: &'a u8, _: &'b u8) -> &'a u8 {
        a
    }
}

fn first<'a, 'b>(a: &'a u8, b: &'b u8) -> &'a u8 {
    let callback = Pair::<'a, 'b>::first;
    let pair = Pair(a, b);
    callback(pair.0, pair.1)
}

fn main() {
    let a = 5;
    let b = 2;
    assert!(Filter(&a).run());
    assert!(!Filter(&b).run());
    assert_eq!(*first(&a, &b), 5);
    println!("inherent callbacks passed");
}
