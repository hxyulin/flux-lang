trait Tr<T,U> {fn take(x:T,y:U)->U;}
struct S;
impl<T,U> Tr<T,U> for S {fn take(_:T,y:U)->U{y}}
fn f<'a,'b,'c>(x:(&'a i32,&'b i32),y:&'c i32)->&'a i32{S::take(x,y)}
fn main() {}
