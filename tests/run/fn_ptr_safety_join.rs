// `if` and `match` join a safe and an unsafe function pointer to
// `unsafe fn`, and the call goes to the selected function.

fn safe(x: i32) -> i32 {
    x + 1
}

unsafe fn not_safe(x: i32) -> i32 {
    x * 10
}

fn pick(c: bool) -> unsafe fn(i32) -> i32 {
    let a: fn(i32) -> i32 = safe;
    let b: unsafe fn(i32) -> i32 = not_safe;
    if c { a } else { b }
}

fn by_arm(n: u8) -> i32 {
    let a: fn(i32) -> i32 = safe;
    let b: unsafe fn(i32) -> i32 = not_safe;
    let f = match n {
        0 => a,
        _ => b,
    };
    unsafe { f(4) }
}

fn main() {
    let x = unsafe { pick(true)(2) };
    let y = unsafe { pick(false)(2) };
    println!("{x} {y} {} {}", by_arm(0), by_arm(1));
    std::process::exit(x + y + by_arm(0));
}
