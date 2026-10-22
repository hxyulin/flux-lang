// What `core`'s macros ask of their arguments: `assert!` a `bool`,
// `assert_eq!` `PartialEq` and `Debug`, `{}` `Display` and `{:?}`
// `Debug`, reported where rustc reports them.

struct S;

#[derive(Debug, PartialEq)]
struct D(i32);

fn main() {
    let d = D(1);
    assert!(1);
    assert_eq!(S, S);
    assert_eq!(d, D(2), "{}", d.0);
    assert_ne!(d, D(3), "{:?}", S);
    let x: u32 = 5;
    debug_assert!(x == 5u8);
    if x > 5 {
        panic!("{}", S);
    }
    if x > 6 {
        unreachable!("{} {}", x, d);
    }
}
