trait Annotate {
    type Value;
    fn annotate(self) -> Result<Self::Value, u8>;
}

impl<T> Annotate for Result<T, u8> {
    type Value = T;

    fn annotate(self) -> Result<T, u8> {
        self
    }
}

fn evaluate(reject: bool) -> Result<(), u8> {
    if reject {
        match Err(7u8).annotate() {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
    }
    Ok(())
}

fn main() {
    println!("{:?} {:?}", evaluate(false), evaluate(true));
    println!("{:?}", parsed_version("42"));
}

fn parsed_version(text: &str) -> (u32, bool) {
    let minor = match text.parse().ok() {
        Some(value) => value,
        None => panic!("invalid version"),
    };
    println!("{minor}");
    (minor, false)
}
