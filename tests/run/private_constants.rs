fn first() -> &'static str {
    "first"
}

fn second() -> &'static str {
    "second"
}

fn shared() -> &'static str {
    "first"
}

static INDIRECT: &str = "second";

fn main() {
    println!("{} {} {} {}", first(), second(), shared(), INDIRECT);
    assert_eq!(first(), shared());
    assert_eq!(second(), INDIRECT);
}
