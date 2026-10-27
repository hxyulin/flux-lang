#[test]
fn main(x: Missing) {
    missing!();
}

#[cfg_attr(all(), test)]
fn value() -> Missing {
    missing!();
}

fn value() -> u32 {
    19
}

macro_rules! tests {
    () => {
        #[test]
        fn value(_: missing!()) {
            missing!();
        }
    };
}
tests!();

mod m {
    #[test]
    pub fn read() -> Missing {
        missing!();
    }
    pub fn read() -> u32 {
        23
    }
}

fn main() {
    #[test]
    fn value() -> Missing {
        missing!();
    }
    println!("{} {}", value(), m::read());
    assert_eq!(value() + m::read(), 42);
}
