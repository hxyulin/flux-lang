macro_rules! shift { ($t:ty, $n:expr) => { 1 as $t << $n } }
macro_rules! inner {
    ((u32)) => { false };
    ($t:ty) => { true };
}
macro_rules! forwarded { ($t:ty) => { inner!($t) } }
macro_rules! name { ($t:ty) => { stringify!($t) } }
fn main() {
    assert_eq!(shift!(u32, 5), 32);
    assert!(forwarded!(u32));
    assert!(!inner!((u32)));
    assert_eq!(name!(u32), "u32");
    println!("{} {}", shift!(u32, 5), name!(u32));
}
