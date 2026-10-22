//! `panic!` with a formatted message, after output: the message and
//! where it was called go to stderr, and the program aborts.

#[derive(Debug)]
struct Config {
    name: &'static str,
    level: u8,
}

fn check(c: &Config, limit: u8) {
    if c.level > limit {
        panic!(
            "level {} of {:?} is over {limit}: {c:#?}",
            c.level, c.name
        );
    }
    println!("{} ok", c.name);
}

fn main() {
    check(&Config { name: "low", level: 1 }, 5);
    check(&Config { name: "high", level: 9 }, 5);
}
