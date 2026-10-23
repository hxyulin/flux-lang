trait Tr<'a,'b> {fn take(x:&'a mut &'a i32,y:&'b mut &'b i32);} struct S; impl<'a> Tr<'a,'a> for S {fn take(_: &'a mut &'a i32,_:&'a mut &'a i32){}} fn f<'a,'b>(x:&'a mut &'a i32,y:&'b mut &'b i32){S::take(x,y);}
fn main() {}
