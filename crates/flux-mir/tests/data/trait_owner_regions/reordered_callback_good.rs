trait Tr<'a,'b>{fn take(x:(&'a i32,&'b i32))->Option<&'a i32>;}
struct S; impl<'x,'y> Tr<'y,'x> for S {fn take(x:(&'y i32,&'x i32))->Option<&'y i32>{Some(x.0)}}
fn f<'a,'b>(x:&'a i32,y:&'b i32)->Option<&'a i32>{[(x,y)].into_iter().find_map(S::take)}
fn main(){}
