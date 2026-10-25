use std::sync::Arc;
struct Tree { cfg: Option<Arc<()>> }
impl Tree {fn cfg_options(&self)->Option<&Arc<()>>{self.cfg.as_ref()}}
#[derive(Clone, Copy)] struct Node<'a>(&'a Tree);
trait View<'a>: Copy { fn syntax(self)->Node<'a>; }
impl<'a> View<'a> for Node<'a> { fn syntax(self)->Node<'a>{self} }
impl<'a> Node<'a> { fn tree(self)->&'a Tree{self.0} }
fn inspect<'a>(values: impl IntoIterator<Item=Node<'a>>) -> Vec<Node<'a>> {
    let mut iter = values.into_iter().peekable();
    let _cfg = iter.peek().and_then(|node| node.syntax().tree().cfg_options()).cloned();
    let mut result=Vec::new();
    for node in iter { result.push(node); }
    result
}
fn main() {
    let tree=Tree{cfg:Some(Arc::new(()))};
    let result=inspect([Node(&tree),Node(&tree)]);
    assert_eq!(result.len(),2);
    assert!(result[0].tree().cfg_options().is_some());
    println!("iterator item regions {}",result.len());
}
