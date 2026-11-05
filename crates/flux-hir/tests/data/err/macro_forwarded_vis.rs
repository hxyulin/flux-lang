macro_rules! literal { ([struct]) => { 1 } }
macro_rules! trailing { ([$k:ident $v:vis]) => { 2 } }
macro_rules! outer {
    ($v:vis $i:ident) => {
        fn forwarded() -> i32 { literal!([$v struct]) }
        fn direct() -> i32 { trailing!([struct]) }
    };
}
outer!(x);
fn main() {}
