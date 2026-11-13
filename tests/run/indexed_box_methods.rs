trait Selection<T> { fn init(&mut self) -> Option<T>; }
struct Counter(u32);
impl Selection<u32> for Counter {
    fn init(&mut self) -> Option<u32> { self.0 += 1; Some(self.0) }
}
fn select<T>(choices: &mut Vec<Box<dyn Selection<T>>>) -> Option<T> {
    choices[0].init()
}
fn main() {
    let mut choices: Vec<Box<dyn Selection<u32>>> = vec![Box::new(Counter(7))];
    assert_eq!(choices[0].init(), Some(8));
    assert_eq!(select(&mut choices), Some(9));
    let inner: Box<dyn Selection<u32>> = Box::new(Counter(19));
    let mut nested: Vec<Box<Box<dyn Selection<u32>>>> = vec![Box::new(inner)];
    assert_eq!(nested[0].init(), Some(20));
    println!("indexed box methods complete");
}
