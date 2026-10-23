trait Tr<'a,T>{fn take(x:&'a i32,y:T)->T;} struct S; impl<'a,T> Tr<'a,T> for S{fn take(_:&'a i32,y:T)->T{y}} fn f<'a,'b>(x:&'a i32,y:&'b i32)->&'a i32{S::take(x,y)}
fn main() {}
