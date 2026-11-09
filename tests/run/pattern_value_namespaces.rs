enum Class { Bytes(u32), Unicode(u32) }
enum Kind { Class(Class), Empty }
fn inspect(kind: Kind) -> u32 {
    use crate::{Class, Kind::*};
    match kind {
        Class(Class::Bytes(value)) => value,
        Class(Class::Unicode(value)) => value + 1,
        Empty => 0,
    }
}
fn main() {
    assert_eq!(inspect(Kind::Class(Class::Bytes(41))), 41);
    println!("{}", inspect(Kind::Class(Class::Unicode(41))));
}
