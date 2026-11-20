fn number<const N: isize>() -> isize {
    N
}

macro_rules! inner {
    ($value:expr) => {
        number::<$value>()
    };
}

macro_rules! forward {
    ($value:expr) => {
        inner!($value)
    };
}

fn main() {
    let result = forward!({ 40 + 2 }) + inner!(-2);
    assert_eq!(result, 40);
    println!("{result}");
}
