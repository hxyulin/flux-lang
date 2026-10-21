// Modules, imports and visibility. `modules` is an inline module whose
// children live in files under `modules/`.

mod modules {
    pub mod shapes;
    pub mod util;
}

mod math {
    pub mod ops {
        pub const fn double(x: i32) -> i32 {
            x * 2
        }

        pub(super) fn halve(x: i32) -> i32 {
            x / 2
        }

        pub(in crate::math) fn negate(x: i32) -> i32 {
            -x
        }

        pub(crate) fn inc(x: i32) -> i32 {
            // A child sees its ancestors' private items.
            super::helper(x) + crate::math::helper(1)
        }

        pub(self) fn quiet() -> i32 {
            0
        }

        pub(in self) fn quieter() -> i32 {
            quiet()
        }

        pub mod deep {
            pub(in crate::math) fn deepest() -> i32 {
                super::super::helper(super::quieter())
            }

            pub(in super::super) fn up() -> i32 {
                3
            }
        }
    }

    fn helper(x: i32) -> i32 {
        x
    }

    pub use self::ops::double;
    pub use ops::double as twice;

    pub fn mixed(x: i32) -> i32 {
        ops::halve(x) + ops::negate(x) + self::ops::inc(x) + ops::deep::deepest() + ops::deep::up()
    }
}

mod glob_a {
    pub fn from_a() -> i32 {
        1
    }

    pub use crate::glob_b::*;
}

mod glob_b {
    pub fn from_b() -> i32 {
        2
    }

    // Globs may import each other.
    pub use crate::glob_a::*;
}

mod reexports {
    mod hidden {
        fn private() -> i32 {
            4
        }

        pub fn exposed() -> i32 {
            private()
        }

        pub struct Token;
    }

    // A glob re-exports only what is visible.
    pub use self::hidden::*;

    pub mod nested {
        // `super::*` sees the parent's private items too.
        use super::*;

        pub fn get() -> i32 {
            exposed() + local()
        }
    }

    fn local() -> i32 {
        5
    }
}

use math::{self as m, mixed, ops::double as dbl};
use modules::shapes::{Point, point::origin};
use modules::util;
use self::glob_a::*;

const LIMIT: i32 = 100;
const DOUBLED: i32 = math::double(LIMIT) + dbl(1);
const WIDE: u8 = u8::MAX;
const BITS: u32 = i64::BITS + u16::BITS;
const SMALLEST: i8 = i8::MIN;
const ABS: i32 = (-5i32).abs();
const POWER: u32 = 3u32.pow(4);
const ONES: u32 = u8::MAX.count_ones();

struct Counter {
    value: i32,
}

impl Counter {
    const START: i32 = 10;
    pub const STEP: i32 = Self::START / 5;

    const fn new() -> Counter {
        Counter { value: Self::START }
    }

    fn bump(self) -> Counter {
        Counter { value: self.value + Counter::STEP }
    }
}

fn main() {
    let a = m::twice(LIMIT) + mixed(DOUBLED) + crate::math::ops::double(1);
    let p = Point::new(1, 2);
    let o = origin();
    let b = util::clamp(p.x + o.y, 0, LIMIT) + util::helpers::three();
    let c = from_a() + from_b() + glob_b::from_a() + glob_a::from_b();
    let d = reexports::exposed() + reexports::nested::get();
    let _t = reexports::Token;
    let e = Counter::new().bump().value + Counter::STEP;
    let f = i32::MAX.wrapping_add(1).min(0) + i32::max(3, 4) + i32::MIN.max(0);
    let g = (WIDE as i32) + (BITS as i32) + (SMALLEST as i32) + ABS + (POWER as i32) + (ONES as i32);
    let _sum = a + b + c + d + e + f + g;
}
