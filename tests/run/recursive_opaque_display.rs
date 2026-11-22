use std::fmt::{self, Display};

fn with_display<F>(mut callback: F) -> String
where
    F: FnMut(&mut dyn FnMut(&dyn Display) -> fmt::Result) -> fmt::Result,
{
    use std::fmt::Write;
    let mut result = String::new();
    callback(&mut |value| write!(&mut result, "{value}")).unwrap();
    result
}

fn display(depth: u32) -> impl Display {
    format_args!("[{}]", with_display(|write| {
        if depth == 0 {
            write(&"leaf")
        } else {
            write(&display(depth - 1))
        }
    }))
    .to_string()
}

fn main() {
    println!("{}", display(3));
    println!("{}", number(true));
}

fn number(recurse: bool) -> impl Display {
    if recurse {
        let value: &dyn Display = &number(false);
        println!("{value}");
    }
    42
}
