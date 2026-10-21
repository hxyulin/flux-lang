// Unresolved paths.

mod m {
    pub fn f() {}

    pub mod n {
        pub fn g() {}

        pub fn up() {
            super::super::super::f();
        }
    }

    pub struct S;
}

struct T;

fn main() {
    m::f();
    m::n::g();
    m::n::up();
    let _s = m::S;
    let _t = T;

    nope::f();
    m::nope::f();
    m::missing();
    let _a = m::MISSING;
    let _b: m::Missing = m::S;
    let _c: nope::Missing = T;
    unknown();
    let _d: Unknown = T;
    crate::nope();
    crate::nope::f();
    self::nope();
    super::f();
    m::crate::f();
    m::super::f();
    m::self::f();
    ::m::f();
    ::f();
    core::nope();
    m();
    let _e = m;
    let _f: m = T;
    let _g = m {};
    let _h = m::n;
    f();
    m::S {};
    m::Nope {};
    T::missing();
    let _i = T::MISSING;
}
