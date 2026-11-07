trait Value {
    type Out;
    fn value(self) -> Self::Out;
}

impl Value for bool {
    type Out = u32;
    fn value(self) -> u32 {
        if self { 7 } else { 3 }
    }
}

impl Value for u8 {
    type Out = u64;
    fn value(self) -> u64 {
        self as u64 + 11
    }
}

fn narrow<T: Value<Out = u32>>(input: T) -> u32 {
    input.value()
}

fn wide<T: Value<Out = u64>>(input: T) -> u64 {
    input.value()
}

fn opaque_narrow() -> impl Value<Out = u32> {
    true
}

fn opaque_wide() -> impl Value<Out = u64> {
    5u8
}

struct Node {
    next: Option<Box<Node>>,
    value: u32,
}

fn require_send<T: Send>(_: &T) -> u32 {
    1
}

fn main() {
    let node = Node {
        next: Some(Box::new(Node { next: None, value: 13 })),
        value: 17,
    };
    let mut total = 0u64;
    for _ in 0..4 {
        total += narrow(false) as u64;
        total += narrow(opaque_narrow()) as u64;
        total += wide(5u8);
        total += wide(opaque_wide());
        total += require_send(&node) as u64;
    }
    assert_eq!(total, 172);
    assert_eq!(node.next.as_ref().unwrap().value, 13);
    println!("{} {}", total, node.value);
}
