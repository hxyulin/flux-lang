use std::sync::OnceLock;
struct Node { next: Option<Box<Node>>, value: u32 }
static ROOT: OnceLock<Node> = OnceLock::new();
fn require<T: Send + Sync>() {}
fn sum(node: &Node) -> u32 {
    let mut total = node.value;
    let mut next = node.next.as_deref();
    while let Some(node) = next { total += node.value; next = node.next.as_deref(); }
    total
}
fn main() {
    require::<Node>();
    let root = ROOT.get_or_init(|| Node { next: Some(Box::new(Node { next: None, value: 7 })), value: 11 });
    assert_eq!(sum(root), 18);
    let value = Node { next: Some(Box::new(Node { next: None, value: 13 })), value: 17 };
    let actual = std::thread::scope(|scope| scope.spawn(move || sum(&value)).join().unwrap());
    assert_eq!(actual, 30);
    println!("recursive auto {} {}", sum(root), actual);
}
