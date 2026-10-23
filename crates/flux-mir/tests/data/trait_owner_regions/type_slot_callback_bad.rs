trait Tr<T> {fn take(x:T)->T;} struct S; impl<T> Tr<T> for S {fn take(x:T)->T{x}} fn f<'a>(x:&'a i32)->Option<&'static i32>{[x].into_iter().map(S::take).next()}
fn main() {}
