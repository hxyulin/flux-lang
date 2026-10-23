trait Tr<'a>{fn take(x:&'a i32)->Option<i32>;} struct S; impl<'a:'static> Tr<'a> for S{fn take(_:&'a i32)->Option<i32>{None}} fn f<'a>(x:&'a i32){S::take(x);}
fn main() {}
