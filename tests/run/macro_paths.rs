//! Macros named by path: `use` imports of `macro_rules!` macros (the
//! 2018 idiom `pub(crate) use m;`), `crate::`, `self::` and `super::`
//! paths to them, and `core::` paths to `core`'s.

unsafe extern "C" {
    fn printf(fmt: *const i8, ...) -> i32;
}

mod inner {
    macro_rules! double {
        ($e:expr) => {
            $e * 2
        };
    }
    pub(crate) use double;

    pub fn f(x: i32) -> i32 {
        self::double!(x)
    }
}

macro_rules! one {
    () => {
        1
    };
}
pub(crate) use one;

mod a {
    use crate::inner::double;

    pub fn g(x: i32) -> i32 {
        crate::one!() + double!(x) + super::one!()
    }
}

fn main() {
    let x = inner::f(3) + a::g(4) + inner::double!(1);
    core::assert!(x == 18);
    ::core::assert_eq!(x, 18);
    unsafe { printf(c"%d\n".as_ptr(), x) };
}
