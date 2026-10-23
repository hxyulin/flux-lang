struct S<'a>(&'a i32); impl<'a> S<'a>{fn take(x:&'a i32)->&'a i32{x}} fn f<'a>(x:&'a i32)->&'a i32{S::take(x)}
fn main() {}
