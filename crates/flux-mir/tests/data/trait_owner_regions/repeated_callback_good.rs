trait Tr<'a,'b>{fn take(x:(&'a mut &'a i32,&'b mut &'b i32))->Option<i32>;} struct S; impl<'x> Tr<'x,'x> for S{fn take(_:(&'x mut &'x i32,&'x mut &'x i32))->Option<i32>{None}} fn f<'a>(x:&'a mut &'a i32,y:&'a mut &'a i32){[(x,y)].into_iter().find_map(S::take);}
fn main() {}
