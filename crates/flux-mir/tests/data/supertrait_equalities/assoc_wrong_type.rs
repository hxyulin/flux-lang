trait P{type A;} trait C<T>:P<A=T>{} struct S; impl P for S{type A=i32;} impl C<u8> for S{} fn main(){}
