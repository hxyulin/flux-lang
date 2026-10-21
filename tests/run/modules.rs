//! Modules across files. The inline module `modules` holds the file
//! modules `modules/geometry.rs` (the `x.rs` layout, with children in
//! `modules/geometry/`) and `modules/util/mod.rs` (the `x/mod.rs` layout),
//! nested two levels further down. Covers `use` in every form (paths,
//! groups, renames, globs, `self`, `super`, `crate`, re-exports, chained
//! re-exports, explicit names shadowing globs), visibility (`pub`,
//! `pub(crate)`, `pub(super)`, `pub(self)`, `pub(in path)`), structs with
//! private fields made by public constructors, impl blocks in another
//! module than their type, constants in modules, and functions, structs
//! and constants that share a name across modules (including functions
//! called `main` and `printf`).

mod modules {
    pub mod geometry;
    pub mod util;

    /// An inline module beside the file modules.
    pub mod report {
        use super::geometry::{self, Matrix, Rect, Round as Circle, apply};
        use super::util::bits::*;
        use super::util::{self as u, Counter, describe as util_describe};
        // An explicit import shadows the `describe` the glob above brings.
        use crate::ffi::{line, title};
        use crate::modules::geometry::describe;

        pub(crate) fn run() -> i64 {
            title(c"report".as_ptr());
            let r = Rect::new(3, 4);
            let c = Circle::new(5);
            let m = Matrix::scale(2, 3).then(&Matrix::IDENTITY);
            let big = apply(&m, &r);
            line(c"big.area()".as_ptr(), big.area() as i64);
            line(c"c.r".as_ptr(), c.r as i64);
            line(c"c.id()".as_ptr(), c.id() as i64);
            line(c"describe()".as_ptr(), describe());
            line(c"util_describe()".as_ptr(), util_describe());
            line(c"bits describe".as_ptr(), super::util::bits::describe());
            line(c"popcount(MASK)".as_ptr(), popcount(MASK) as i64);
            line(c"parity(HIGH)".as_ptr(), parity(HIGH) as i64);
            line(c"geometry::secret()".as_ptr(), geometry::secret());
            line(c"u::only_parent()".as_ptr(), u::only_parent());
            let mut counter = Counter::new(7);
            line(c"census".as_ptr(), geometry::census(&mut counter));
            line(c"counter.count()".as_ptr(), counter.count() as i64);
            line(c"total_area".as_ptr(), geometry::total_area(&r, &big));
            big.area() as i64 + c.r as i64
        }
    }
}

mod ffi {
    unsafe extern "C" {
        pub fn printf(format: *const i8, ...) -> i32;
    }

    pub fn line(label: *const i8, v: i64) {
        unsafe {
            printf(c"  %s = %lld\n".as_ptr(), label, v);
        }
    }

    pub fn title(name: *const i8) {
        unsafe {
            printf(c"%s\n".as_ptr(), name);
        }
    }
}

/// A Rust function called `printf` and one called `putchar`, which must
/// not be confused with the C functions.
mod shout {
    pub fn printf(x: i32) -> i32 {
        x * 2 + 1
    }

    pub fn putchar(c: i32) -> i32 {
        printf(c) - 1
    }
}

use self::modules::geometry::transform;
use crate::modules::util::bits;
use ffi::{line, title};
use modules::geometry::{self as geo, Matrix, Rect, Round, shapes::Point as GeoPoint};
use modules::util::{
    self, Counter, LIMIT, LeafRing, Point as UtilPoint, bits::popcount, clamp, ring, tally,
};

const LIMIT_TWICE: i32 = LIMIT * 2;
// Constants of structs with private fields, from `const fn`s and
// associated constants of other modules.
const ROUND: Round = Round::new(LIMIT_TWICE);
const MATRIX: Matrix = Matrix::IDENTITY;
const SIDE: i32 = Rect::MAX_SIDE / LIMIT + geo::IDENTITY_SCALE;

/// The same name as `describe` in four other modules.
fn describe() -> i64 {
    5
}

fn main() {
    title(c"names".as_ptr());
    line(c"describe()".as_ptr(), describe());
    line(c"geo::describe()".as_ptr(), geo::describe());
    line(c"shapes::describe()".as_ptr(), geo::shapes::describe());
    line(c"util::describe()".as_ptr(), util::describe());
    line(c"bits::describe()".as_ptr(), bits::describe());
    line(c"geo::main()".as_ptr(), geo::main());
    line(c"more::describe()".as_ptr(), geo::extra::more::describe());
    line(c"shout::printf(20)".as_ptr(), shout::printf(20) as i64);
    line(c"shout::putchar(20)".as_ptr(), shout::putchar(20) as i64);
    line(
        c"crate path".as_ptr(),
        crate::modules::util::bits::describe() * 10 + self::describe(),
    );

    title(c"consts".as_ptr());
    line(c"LIMIT".as_ptr(), LIMIT as i64);
    line(c"LIMIT_TWICE".as_ptr(), LIMIT_TWICE as i64);
    line(c"util::LIMIT".as_ptr(), util::LIMIT as i64);
    line(c"bits::LIMIT".as_ptr(), bits::LIMIT as i64);
    line(c"bits::MASK".as_ptr(), bits::MASK as i64);
    line(c"geo::ORIGIN_TAG".as_ptr(), geo::ORIGIN_TAG);
    line(c"Rect::MAX_SIDE".as_ptr(), Rect::MAX_SIDE as i64);
    line(c"Rect::UNIT.area()".as_ptr(), Rect::UNIT.area() as i64);
    line(c"ROUND.id()".as_ptr(), ROUND.id() as i64);
    line(c"ROUND.r".as_ptr(), ROUND.r as i64);
    line(c"MATRIX.sx()".as_ptr(), MATRIX.then(&MATRIX).sx() as i64);
    line(c"SIDE".as_ptr(), SIDE as i64);
    line(
        c"IDENTITY_SCALE".as_ptr(),
        transform::IDENTITY_SCALE as i64 + geo::IDENTITY_SCALE as i64,
    );

    title(c"structs".as_ptr());
    let mut r = Rect::new(2000, -5);
    line(c"clamped width".as_ptr(), r.width() as i64);
    line(c"clamped height".as_ptr(), r.height() as i64);
    r = geo::Rect::new(6, 7);
    r.widen(4);
    line(c"widened width".as_ptr(), r.width() as i64);
    let t = r.transposed();
    line(c"t.width()".as_ptr(), t.width() as i64);
    line(c"checksum(t)".as_ptr(), transform::checksum(&t) as i64);
    let p = GeoPoint::new(-3, 4);
    let GeoPoint { x, y } = GeoPoint::new(p.x * 2, p.y + 1);
    line(c"p.norm1()".as_ptr(), p.norm1() as i64);
    line(c"x * y".as_ptr(), (x * y) as i64);
    let q = UtilPoint::new(40, 2);
    let UtilPoint(a, b) = UtilPoint(q.0 + 1, q.1);
    line(c"q.sum()".as_ptr(), q.sum());
    line(c"a - b".as_ptr(), a - b);
    let mut counter = Counter::new(3);
    counter.step += 2;
    counter.bump();
    counter.bump();
    line(c"counter.count()".as_ptr(), counter.count() as i64);
    line(c"tally::tally(4)".as_ptr(), tally::tally(4));
    line(c"tally()".as_ptr(), tally());
    line(c"clamp".as_ptr(), clamp(LIMIT_TWICE, 0, LIMIT) as i64);
    line(c"popcount".as_ptr(), popcount(u64::MAX >> 3) as i64);
    let LeafRing { size } = ring();
    line(c"ring()".as_ptr(), size);
    let applied = geo::tf::apply(&Matrix::scale(SIDE, 2), &Rect::UNIT);
    line(c"tf::apply".as_ptr(), applied.area() as i64);
    unsafe {
        crate::ffi::printf(c"  direct printf = %d\n".as_ptr(), shout::printf(SIDE));
    }

    let total = modules::report::run();
    title(c"done".as_ptr());
    line(c"total".as_ptr(), total);
}
