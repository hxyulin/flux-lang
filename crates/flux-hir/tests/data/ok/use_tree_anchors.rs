// `super` after `self::` or `super::` in a use group continues the anchor.
mod a {
    pub fn g() -> i32 {
        1
    }
    pub mod error {
        pub struct Error;
    }
    pub mod b {
        pub mod blend {
            pub struct State;
        }
        use self::{blend::State, super::g as h};
        pub mod c {
            use super::{super::error::Error, blend::State};
            use super::{super::{super::a::g}};
            pub fn k() -> (Error, State, i32) {
                (Error, State, g())
            }
        }
        pub fn f() -> (State, i32) {
            (State, h())
        }
    }
}

fn main() {
    let _ = a::b::f();
    let _ = a::b::c::k();
}
