//! `Box<dyn Trait>` and `Box<[T]>`: boxes coerced to trait objects and
//! slices (`CoerceUnsized`), called through their vtables, dropped
//! through the vtable's drop function and freed with the size it
//! records, `size_of_val` of both, and boxed slices to and from `Vec`.

trait Shape {
    fn area(&self) -> i32;
    fn name(&self) -> String {
        String::from("shape")
    }
}
struct Sq(i32);
struct Named(String, i32);
impl Shape for Sq {
    fn area(&self) -> i32 { self.0 * self.0 }
}
impl Shape for Named {
    fn area(&self) -> i32 { self.1 }
    fn name(&self) -> String { self.0.clone() }
}
impl Drop for Named {
    fn drop(&mut self) { println!("drop {}", self.0); }
}
fn total(shapes: &[Box<dyn Shape>]) -> i32 {
    shapes.iter().map(|s| s.area()).sum()
}
fn main() {
    let a: Box<dyn Shape> = Box::new(Sq(3));
    println!("{} {}", a.area(), a.name());
    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Sq(2)), Box::new(Named(String::from("n1"), 7)), a];
    println!("{}", total(&shapes));
    for s in &shapes {
        println!("{} {}", s.name(), size_of_val(&**s));
    }
    drop(shapes);
    let b: Box<[i32]> = Box::new([1, 2, 3]);
    println!("{:?} {} {}", b, b.len(), size_of_val(&*b));
    let n: Box<dyn Shape> = Box::new(Named(String::from("last"), 1));
    println!("{}", n.area());

    let mut v = Vec::with_capacity(10);
    v.extend([1, 2, 3]);
    let b: Box<[i32]> = v.into_boxed_slice();
    println!("{:?} {}", b, b.len());
    let mut w = b.into_vec();
    w.push(4);
    println!("{w:?} {}", w.capacity());
    let c: Box<[String]> = Box::from(vec![String::from("a"), String::from("b")]);
    println!("{c:?} {}", c[1]);
    let e: Box<[u8]> = Vec::new().into_boxed_slice();
    println!("{}", e.len());
}
