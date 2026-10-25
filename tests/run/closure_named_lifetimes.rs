struct Node<'t> { text: &'t str }

fn collect<'t>(text: &'t str) -> Vec<&'t str> {
    let mut out = Vec::new();
    let push = |node: Node<'t>, out: &mut Vec<&'t str>| { out.push(node.text); };
    push(Node { text }, &mut out);
    out
}

fn main() {
    let text = String::from("borrowed");
    let values = collect(&text);
    let identity = |how: &'static str| how;
    let first = |values: Vec<(&'static str, u32)>| values[0].1;
    visit(&mut |node, out| { out.push(node.text.into()); node.text.len() });
    update(&mut |ctx, amount| { ctx.count += amount; ctx.text.len() + ctx.count });
    println!("{} {} {}", values[0], identity("static"), first(vec![("key", 7)]));
}

fn visit(f: &mut dyn FnMut(Node<'_>, &mut Vec<String>) -> usize) {
    let text = String::from("node");
    let mut out = Vec::new();
    assert_eq!(f(Node { text: &text }, &mut out), 4);
    assert_eq!(out[0], "node");
}

struct Ctx<'a> { text: &'a str, count: usize }
fn update(f: &mut dyn FnMut(&mut Ctx<'_>, usize) -> usize) {
    let text = String::from("context");
    let mut ctx = Ctx { text: &text, count: 0 };
    assert_eq!(f(&mut ctx, 3), 10);
    assert_eq!(ctx.count, 3);
}
