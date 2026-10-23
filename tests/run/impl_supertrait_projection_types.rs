trait Parent {
    fn read(self, n: i32) -> i32;
}
trait Child: Parent {}
struct Callable<F>(F);
impl<F: Fn(i32) -> i32> Parent for Callable<F> {
    fn read(self, n: i32) -> i32 { (self.0)(n) }
}
impl<F: Fn(i32) -> i32> Child for Callable<F> {}
struct Borrowed<F>(F);
impl<F> Parent for Borrowed<F> {
    fn read(self, n: i32) -> i32 { n + 1 }
}
// This higher-ranked bound is unrelated to proving Parent.
impl<F: Fn(&i32) -> i32> Child for Borrowed<F> {}
fn read<T: Child>(x: T, n: i32) -> i32 { x.read(n) }
fn borrow(n: &i32) -> i32 { *n + 2 }
fn main() {
    let offset = 7;
    println!("{}", read(Callable(|n| n + offset), 35));
    println!("{}", read(Borrowed(borrow), 41));
}
