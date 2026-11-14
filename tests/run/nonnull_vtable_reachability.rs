use std::ptr::NonNull;

trait Value {
    fn value(&self) -> i32;
}
struct Number(i32);
impl Value for Number {
    fn value(&self) -> i32 { self.0 }
}
fn main() {
    let mut number = Number(42);
    let concrete = NonNull::from(&mut number);
    let object: NonNull<dyn Value> = concrete;
    // SAFETY: number remains live and the pointer is only read here.
    println!("{}", unsafe { object.as_ref() }.value());
}
