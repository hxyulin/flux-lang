trait Tr<'a,'b> {fn take<'c>(x:&'a i32,y:&'b i32,z:&'c i32)->(&'a i32,&'c i32);} struct S; impl<'x,'y> Tr<'y,'x> for S {fn take<'z>(x:&'y i32,_:&'x i32,z:&'z i32)->(&'y i32,&'z i32){(x,z)}} fn f<'a,'b,'c>(x:&'a i32,y:&'b i32,z:&'c i32)->(&'b i32,&'c i32){S::take(x,y,z)}
fn main() {}
