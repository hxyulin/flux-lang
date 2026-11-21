fn index(label: &str, value: usize) -> usize {
    println!("{label}");
    value
}

fn right_side() -> [String; 3] {
    println!("right");
    ["first".into(), "skipped".into(), "last".into()]
}

fn main() {
    let mut values = [String::from("a"), String::from("b")];
    [values[index("left 1", 1)], .., values[index("left 0", 0)]] = right_side();
    println!("{values:?}");

    let mut x = 0;
    let mut y = 0;
    println!("{x} {y}");
    ([(x, _), (_, y)]) = [(1, 2), (3, 4)];
    println!("{x} {y}");
}
