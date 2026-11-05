macro_rules! not_an_item {
    () => {
        let x = 1;
    };
}
macro_rules! a_struct {
    () => {
        struct Inner;
    };
}
trait Statement {
    not_an_item!();
}
trait Nested {
    a_struct!();
}
trait Unknown {
    missing!();
}
fn main() {}
