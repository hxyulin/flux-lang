macro_rules! token {
    ($) => { 1u8 };
    ([$]) => { 2u8 };
    (($)) => { 3u8 };
    ($name:ident) => { 4u8 };
}
macro_rules! repeated {
    ($( $ )*) => { 5u8 };
}
fn main() {
    println!("{} {} {} {} {} {}", token!($), token!([$]), token!(($)),
        token!(name), repeated!(), repeated!($ $ $));
}
