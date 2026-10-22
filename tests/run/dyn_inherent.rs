//! Inherent `impl` blocks for trait objects (`impl dyn Shape`, with a
//! lifetime bound too), whose methods call the trait's through the
//! vtable, by method call and by path.

trait Shape { fn area(&self) -> i32; }
struct Sq(i32);
impl Shape for Sq { fn area(&self) -> i32 { self.0 * self.0 } }
impl dyn Shape {
    fn doubled(&self) -> i32 { self.area() * 2 }
    fn describe(&self, extra: i32) -> i32 { self.doubled() + extra }
}
impl<'a> dyn Shape + 'a {
    fn plus(&self, n: i32) -> i32 { self.area() + n }
}
fn main() {
    let s = Sq(3);
    let d: &dyn Shape = &s;
    println!("{} {} {}", d.doubled(), d.describe(1), d.plus(100));
    println!("{}", <dyn Shape>::doubled(d));
}
