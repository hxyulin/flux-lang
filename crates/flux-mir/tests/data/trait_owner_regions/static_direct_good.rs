trait Tr<'a> {fn take(x:&'a i32);} struct S; impl Tr<'static> for S {fn take(_: &'static i32){}} fn f(x:&'static i32){S::take(x);}
fn main() {}
