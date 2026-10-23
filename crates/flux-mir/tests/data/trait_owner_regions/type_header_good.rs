trait Tr<T>{fn take(x:T);}
struct S; impl Tr<&'static i32> for S {fn take(_:&'static i32){}}
fn f(x:&'static i32){S::take(x)}
fn main(){}
