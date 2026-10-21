// Arithmetic that would overflow in branches a known condition rules out
// is not linted, as in rustc.

pub fn dead_literal() {
    if false {
        let _ = 255u8 + 1;
    }
}

pub fn dead_local() {
    if false {
        let x: u8 = 255;
        let _ = x + 1;
    }
}

pub fn known_condition() -> i32 {
    let y = 10;
    if y > 20 {
        return 1 / (y - 10);
    }
    let c = false;
    if c {
        let _ = i32::MAX + 1;
    }
    0
}

pub fn main() {
    dead_literal();
    dead_local();
    let _ = known_condition();
}
