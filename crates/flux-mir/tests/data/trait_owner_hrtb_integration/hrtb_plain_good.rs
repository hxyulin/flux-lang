fn get(x:&i32)->&i32{x} fn need<F:for<'a> Fn(&'a i32)->&'a i32>(_:F){} fn f(){need(get);}
fn main() {}
