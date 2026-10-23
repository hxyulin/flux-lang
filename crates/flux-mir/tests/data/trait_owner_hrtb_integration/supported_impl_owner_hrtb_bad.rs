struct S<'a>(&'a i32); impl<'a> S<'a>{fn get(x:&'a i32)->&'a i32{x}} fn need<F:for<'a> Fn(&'a i32)->&'a i32>(_:F){} fn f(){need(S::get);} fn main() {}
