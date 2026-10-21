// Private items named from outside their module.

mod m {
    fn private_fn() {}
    struct Braced {}
    struct Unit;
    pub struct Tuple(i32);
    pub struct Mixed(pub i32, i32);
    pub struct Open(pub i32);
    const C: i32 = 1;
    mod inner {
        pub fn f() {}
    }

    pub mod n {
        pub(super) fn only_m() {}
        pub(in crate::m) fn also_only_m() {}
    }

    pub fn uses() {
        n::only_m();
        n::also_only_m();
        let _ = Braced {};
        let _ = Unit;
        let _ = Tuple(1);
        let _ = Mixed(1, 2);
        let _ = C;
    }

    pub struct S;

    impl S {
        fn new() -> S {
            S
        }

        pub fn make() -> S {
            S::new()
        }

        const X: i32 = 1;

        pub fn x() -> i32 {
            Self::X
        }

        fn method(&self) {}

        pub fn call(&self) {
            self.method()
        }
    }

    use self::n::only_m;
    pub(crate) fn crate_fn() {
        only_m();
    }
}

mod reexport {
    fn private() {}
    pub(crate) fn crate_only() {}
    mod hidden {}
    pub(crate) mod crate_mod {}
    struct Hidden;

    pub use self::private as exposed;
    pub use self::crate_only as also_exposed;
    pub use self::hidden as hidden_mod;
    pub use self::crate_mod as open_mod;
    pub use self::Hidden as Visible;

    pub fn uses() {
        private();
        crate_only();
        let _ = Hidden;
    }
}

use m::private_fn;

fn main() {
    m::uses();
    m::crate_fn();
    reexport::uses();
    let _ = m::S::make();
    let _ = m::S::x();
    m::S.call();
    m::private_fn();
    let _a = m::Braced {};
    let _b: m::Braced = m::Braced {};
    let _c = m::Unit;
    let _d = m::Tuple(1);
    let _e = m::Mixed(1, 2);
    let _f = m::Open(1);
    let m::Tuple(_) = m::Tuple(2);
    let _g = m::C;
    m::inner::f();
    m::n::only_m();
    m::n::also_only_m();
    let _h = m::S::new();
    let _i = m::S::X;
    m::S::method(&m::S);
    private_fn();
}
