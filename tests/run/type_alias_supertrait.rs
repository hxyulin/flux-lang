#![allow(type_alias_bounds)]

trait Source {
    type Error;
}

trait Additional: Source {}

type Failure<T: Additional> = T::Error;

trait Inner {
    type End;
}

type Nested<T: Additional> = <T::Error as Inner>::End;
type Advisory<T: Additional> = Option<T>;

struct Marker;

impl Source for Marker {
    type Error = Self;
}

impl Additional for Marker {}

impl Inner for Marker {
    type End = u32;
}

fn main() {
    let _: Failure<Marker> = Marker;
    let value: Nested<Marker> = 42;
    let ignored_bound: Advisory<u8> = Some(7);
    assert_eq!(ignored_bound, Some(7));
    println!("{value}");
}
