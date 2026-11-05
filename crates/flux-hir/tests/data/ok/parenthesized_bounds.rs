// Bounds may be parenthesized: `F: (FnOnce() -> T)`.
fn call<T, F>(f: F) -> Option<T>
where
    F: (FnOnce() -> T),
{
    Some(f())
}

fn show<T: (?Sized) + (std::fmt::Debug)>(t: &T) -> String {
    format!("{t:?}")
}

fn boxed(f: Box<dyn (Fn(u8) -> u8)>) -> u8 {
    f(2)
}

fn opaque() -> impl (Fn(u8) -> u8) {
    |x| x + 1
}

fn main() {
    println!("{:?}", call(|| 3));
    println!("{}", show("str"));
    println!("{}", boxed(Box::new(|x| x * 4)));
    println!("{}", opaque()(9));
}
