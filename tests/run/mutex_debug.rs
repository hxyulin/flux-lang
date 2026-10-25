use std::sync::Mutex;

fn main() {
    let value = Mutex::new(vec![7, 11]);
    println!("{:?}", value);
    let mut guard = value.lock().unwrap();
    println!("{:?}", value);
    guard.push(13);
    drop(guard);
    println!("{:?}", value);
    println!("{:#?}", Mutex::new(42));
}
