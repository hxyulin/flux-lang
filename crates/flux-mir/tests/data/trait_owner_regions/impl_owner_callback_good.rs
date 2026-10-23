struct S<'a>(&'a i32); impl<'a> S<'a>{fn take(x:&'a i32)->&'a i32{x}} fn f<'a>(x:&'a i32)->Option<&'a i32>{[x].into_iter().map(S::take).next()}
fn main() {}
