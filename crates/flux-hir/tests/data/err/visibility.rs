// Visibility restrictions, misplaced visibility, missing module files and
// constants without a body. (A constant without a type is a syntax error
// in flux, so it cannot be a fixture here.)

mod before {}

mod a {
    pub(in crate::before) fn not_an_earlier_ancestor() {}
    pub(in crate::b) fn not_an_ancestor() {}
    pub(in a) fn relative() {}
    pub(in crate::nope) fn unresolved() {}
    pub(in self) fn fine() {}

    pub fn uses() {
        not_an_earlier_ancestor();
        not_an_ancestor();
        relative();
        unresolved();
        fine();
    }
}

mod b {}

pub(super) fn above_root() {}

struct S;

pub impl S {}

mod missing;

const NO_BODY: i32;

impl S {
    const ASSOC_NO_BODY: i32;
}

use self;
use crate;

fn main() {
    a::uses();
    above_root();
    let _s = S;
    let _n = NO_BODY + S::ASSOC_NO_BODY;
}
