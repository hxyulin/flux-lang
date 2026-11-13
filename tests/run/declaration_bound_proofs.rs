trait Value {
    fn value(&self) -> usize;
}
impl<T: Copy> Value for T {
    fn value(&self) -> usize {
        1
    }
}
struct Local;
impl Value for Local {
    fn value(&self) -> usize {
        2
    }
}
impl Value for str {
    fn value(&self) -> usize {
        self.len()
    }
}
trait Measure {
    fn measure<T: ?Sized>(value: &T) -> usize;
}
impl Measure for Local {
    fn measure<T: ?Sized>(value: &T) -> usize {
        std::mem::size_of_val(value)
    }
}
struct Storage<T: ?Sized>(Box<T>);
impl<T: ?Sized> Drop for Storage<T> {
    fn drop(&mut self) {
        println!("drop {}", std::mem::size_of_val(&*self.0));
    }
}
trait Marker {}
impl<T: ?Sized> Marker for Box<T> {}
struct Need<T: Marker>(T);
fn inspect<T: ?Sized>(value: &Need<Box<T>>) -> usize {
    std::mem::size_of_val(&*value.0)
}
struct Node {
    next: Option<Box<Node>>,
}
fn send<T: Send>(_: &T) {}
fn main() {
    println!("{} {} {}", 7u8.value(), Local.value(), "hello".value());
    println!("measure {}", Local::measure("hello"));
    let storage = Storage(Box::<str>::from("abc"));
    let value = Need(Box::<str>::from("hello"));
    println!("inspect {}", inspect(&value));
    let node = Node {
        next: Some(Box::new(Node { next: None })),
    };
    send(&node);
    println!("node {}", node.next.is_some());
    drop(storage);
}
