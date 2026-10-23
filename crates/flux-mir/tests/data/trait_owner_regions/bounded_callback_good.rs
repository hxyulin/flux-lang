trait Tr<'a>{fn take(x:&'a i32)->Option<i32>;} struct S; impl<'a:'static> Tr<'a> for S{fn take(_:&'a i32)->Option<i32>{None}} fn f(x:&'static i32){[x].into_iter().find_map(S::take);}
fn main() {}
