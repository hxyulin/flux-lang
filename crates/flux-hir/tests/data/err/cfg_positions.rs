#![cfg(all())]

#[cfg_attr(all(), inline)]
fn conditional_attribute() {}

mod inner {
    #![cfg(all())]
}

struct S {
    #[cfg(any())]
    field: i32,
}

enum E {
    #[cfg(any())]
    A,
}

fn parameters(#[cfg(any())] value: i32) {
    #[cfg(any())]
    let local = value;
    #[cfg(any())]
    value;
}
