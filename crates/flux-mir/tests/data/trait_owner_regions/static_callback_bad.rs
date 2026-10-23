trait Tr<'a> {fn take(x:&'a i32);} struct S; impl Tr<'static> for S {fn take(_: &'static i32){}} fn f<'a>(x:&'a i32){[x].into_iter().for_each(S::take);}
fn main() {}
