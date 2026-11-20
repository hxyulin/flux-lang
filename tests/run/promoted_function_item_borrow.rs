struct Filter<'a>(&'a dyn Fn(u32) -> bool);

fn fits(value: u32) -> bool {
    value == 42
}

fn filter<'a>() -> Filter<'a> {
    Filter(&fits)
}

fn main() {
    assert!((filter().0)(42));
    assert!(!(filter().0)(41));
    println!("{}", (filter().0)(42));
}
