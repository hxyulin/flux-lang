// Branches of types `fn()` and `unsafe fn()` join to `unsafe fn()`:
// the safe pointer coerces to the unsafe one, in either order.

fn safe() {}

unsafe fn not_safe() {}

fn branches(c: bool) {
    let a: fn() = safe;
    let b: unsafe fn() = not_safe;
    let p = if c { a } else { b };
    let q = if c { b } else { a };
    unsafe {
        p();
        q();
    }
}

fn arms(n: i32) {
    let a: fn() = safe;
    let b: unsafe fn() = not_safe;
    let p = match n {
        0 => a,
        _ => b,
    };
    unsafe { p() }
}

fn main() {
    branches(true);
    arms(0);
}
