#[derive(Clone,Copy)] struct Node<'a>(&'a i32); trait Ast<'a>: Copy {fn cast(n:Node<'a>)->Option<Self>; fn syntax(self)->Node<'a>;} impl<'a> Ast<'a> for Node<'a> {fn cast(n:Node<'a>)->Option<Self>{Some(n)} fn syntax(self)->Node<'a>{self}} fn f<'a,N:Ast<'a>>(n:Node<'a>)->Option<N>{N::cast(n)}
fn main() {}
