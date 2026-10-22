//! Items inside function bodies: functions (nested and recursive),
//! structs with inherent and trait `impl` blocks, constants, a `static
//! mut`, an enum, a type alias and a `use`, visible throughout their
//! block, before their declaration too; an inner item shadowing an outer
//! one in its block; and items in generic functions and methods.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn helper() -> i32 {
    1
}

trait Area {
    fn area(&self) -> i32;
}

fn shapes(side: i32) -> i32 {
    // Used before it is declared, as items are visible in their block.
    let s = Square { side };
    struct Square {
        side: i32,
    }
    impl Area for Square {
        fn area(&self) -> i32 {
            self.side * self.side
        }
    }
    impl Square {
        const SIDES: i32 = 4;
        fn perimeter(&self) -> i32 {
            Self::SIDES * self.side
        }
    }
    s.area() + s.perimeter()
}

fn shadowing() -> i32 {
    // An inner item shadows the outer one in its block only.
    let outer = helper();
    let inner = {
        fn helper() -> i32 {
            10
        }
        helper()
    };
    outer + inner + helper()
}

fn nested(n: u32) -> u32 {
    fn fact(n: u32) -> u32 {
        fn one() -> u32 {
            1
        }
        if n == 0 { one() } else { n * fact(n - 1) }
    }
    const BASE: u32 = 100;
    static mut CALLS: u32 = 0;
    unsafe {
        CALLS += 1;
    }
    use core::convert::identity;
    enum Op {
        Add(u32),
        Twice,
    }
    type Num = u32;
    let op = Op::Add(BASE);
    let extra: Num = match op {
        Op::Add(x) => x,
        Op::Twice => 2,
    };
    let twice = match Op::Twice {
        Op::Add(_) => 0,
        Op::Twice => 2,
    };
    identity(fact(n)) + extra + twice + unsafe { CALLS }
}

fn generic<T: Copy>(x: T) -> (T, i32) {
    fn plain() -> i32 {
        7
    }
    (x, plain())
}

struct Counter;

impl Counter {
    fn count(&self) -> i32 {
        fn base() -> i32 {
            3
        }
        base() + 1
    }
}

fn main() {
    let (g, p) = generic(5u8);
    unsafe {
        printf(
            c"%d %d %u %d %d %d\n".as_ptr(),
            shapes(3),
            shadowing(),
            nested(5),
            g as i32,
            p,
            Counter.count(),
        );
    }
}
