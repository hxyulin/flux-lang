use std::sync::atomic::{AtomicUsize, Ordering};
static DROPS: AtomicUsize = AtomicUsize::new(0);
#[derive(Debug)]
struct Value(u32);
impl Drop for Value {
    fn drop(&mut self) {
        DROPS.fetch_add(1, Ordering::SeqCst);
    }
}
#[derive(Debug)]
enum Tree<I, C = u32> {
    Base(I),
    Stack { base: Box<Self>, stack: Vec<C> },
    Alt(Vec<Self>),
}
fn total(tree: &Tree<Value>) -> u32 {
    match tree {
        Tree::Base(value) => value.0,
        Tree::Stack { base, stack } => total(base) + stack.iter().copied().sum::<u32>(),
        Tree::Alt(children) => children.iter().map(total).sum(),
    }
}
struct Borrowed<'a> {
    value: &'a u32,
    children: Vec<Self>,
}
struct Fixed<const N: usize> {
    value: [u8; N],
    children: Vec<Self>,
}
fn main() {
    {
        let tree = Tree::Alt(vec![
            Tree::Base(Value(3)),
            Tree::Stack {
                base: Box::new(Tree::Alt(vec![Tree::Base(Value(5)), Tree::Base(Value(7))])),
                stack: vec![11, 13],
            },
        ]);
        assert_eq!(total(&tree), 39);
        println!("adt self {:?}", tree);
    }
    assert_eq!(DROPS.load(Ordering::SeqCst), 3);
    let constructor: fn(Vec<Tree<Value>>) -> Tree<Value> = Tree::Alt;
    assert_eq!(total(&constructor(Vec::new())), 0);
    let a = 17;
    let b = 19;
    let borrowed = Borrowed {
        value: &a,
        children: vec![Borrowed {
            value: &b,
            children: Vec::new(),
        }],
    };
    assert_eq!(*borrowed.children[0].value, 19);
    let fixed = Fixed::<2> {
        value: [23, 29],
        children: vec![Fixed {
            value: [31, 37],
            children: Vec::new(),
        }],
    };
    assert_eq!(fixed.children[0].value[1], 37);
    println!(
        "owners {} {} {}",
        *borrowed.value,
        fixed.value[1],
        DROPS.load(Ordering::SeqCst)
    );
}
