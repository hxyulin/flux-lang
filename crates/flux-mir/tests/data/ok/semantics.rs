//! Short-circuiting, division checks, compound assignment, and reads that
//! later operands cannot change.

fn logic(a: bool, b: bool) -> bool {
    a && b || !a
}

fn divide(x: i32, y: i32, u: u8) -> i32 {
    x / y + x % y + (u / 3) as i32
}

fn compound(mut x: i64) -> i64 {
    x += 1;
    x <<= 2u8;
    x
}

fn evaluation_order() -> i32 {
    let mut x = 1;
    x + {
        x = 10;
        x
    }
}

fn deferred(flag: bool) -> u32 {
    let x;
    if flag {
        x = 1;
    } else {
        x = 2;
    }
    x
}
