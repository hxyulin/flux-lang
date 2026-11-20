// Fixed item slots remain usable through type parameters and Iterator::Item.
struct S<'a>(&'a i32);
impl<'a> S<'a> {
    fn get(x: &'a i32) -> &'a i32 { x }
    fn cast(x: &'a i32) -> Option<Self> { Some(S(x)) }
}
fn apply<T, F: Fn(T) -> T>(x: T, f: F) -> T { f(x) }
fn identity(x: &i32) -> &i32 { x }
fn higher<F: for<'a> Fn(&'a i32) -> &'a i32>(f: F, x: &i32) -> &i32 { f(x) }
fn cast_first<'a>(values: &[&'a i32]) -> Option<S<'a>> {
    values.iter().copied().find_map(S::cast)
}

struct Node<'a>(&'a str);

impl<'a> Node<'a> {
    fn supports(&self) -> bool {
        !self.0.is_empty()
    }

    fn label(&self) -> &'a str {
        self.0
    }
}

fn checked<'a>(node: Option<Node<'a>>) -> Option<Node<'a>> {
    node.filter(Node::supports)
}

fn fetch<'a>(node: &Node<'a>, getter: fn(&Node<'a>) -> &'a str) -> &'a str {
    getter(node)
}

fn label<'a>(node: &Node<'a>) -> &'a str {
    fetch(node, Node::label)
}

fn main() {
    let x = 37;
    println!("{}", apply(&x, S::get));
    println!("{}", [&x].into_iter().find_map(S::cast).unwrap().0);
    println!("{}", cast_first(&[&x]).unwrap().0);
    println!("{}", higher(identity, &x));
    let name = String::from("node");
    let node = checked(Some(Node(&name))).unwrap();
    println!("{}", label(&node));
}
