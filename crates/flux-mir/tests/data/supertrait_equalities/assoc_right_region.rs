trait P{type A;} trait C<T>:P<A=T>{} struct S<'a>(&'a i32); impl<'a>P for S<'a>{type A=&'a i32;} impl<'a>C<&'a i32> for S<'a>{} fn main(){}
