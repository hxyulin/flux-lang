static ONE: i32 = 1;
static NO_VALUE: i32;

mod m {
    static HIDDEN: i32 = 2;
}

fn shadow(ONE: i32) {}

fn main() {
    let ONE = 2;
    match 5 {
        ONE => {}
        _ => {}
    }
    let h = m::HIDDEN;
}
