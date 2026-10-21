// Unresolved, duplicate and ambiguous imports.

mod a {
    pub fn f() {}
    pub fn g() {}
    pub struct S;
    pub mod inner {}
    pub const N: i32 = 1;
}

mod b {
    pub fn f() {}
    pub struct S;
    pub mod inner {}
}

mod c {
    pub fn f() {}
    pub fn g() {}
}

mod empty {}

use nope::x;
use empty::x;
use empty::y::z;
use crate::missing;
use self::absent;
use missing_crate;
use a::{f, nothing};
use empty::{p, q};
use super::up;
use a::f::{self};
use a::g::*;
use a::S::new;

use a::S;
use b::S;
use a::inner;
use b::inner;
use a::g as h;
use c::g as h;
use a::{N as K, N as K};

use a::N;
const N: i32 = 2;

fn local() {}
use c::f as local;

use empty::*;
mod empty {}

mod globs {
    pub use crate::a::*;
    pub use crate::c::*;

    pub fn call() {
        g();
    }
}

mod cycle_a {
    pub use crate::cycle_b::r;
}

mod cycle_b {
    pub use crate::cycle_a::r;
}

mod clash {}
mod clash {}
struct Dup;
mod Dup {}

struct Twice;

impl Twice {
    fn same() {}
}

impl Twice {
    fn same() {}
}

fn main() {
    f();
    globs::call();
    h();
    Twice::same();
}
