//! Inference: literal defaulting, propagation, `!`, loops and casts.

unsafe extern "C" {
    fn abort() -> !;
}

fn defaults() -> i64 {
    let a = 1;
    let b = 2;
    let c = b + 3;
    let wide: i64 = 7;
    let narrow = 200u8 as i64;
    (a as i64) + (c as i64) + wide + narrow
}

fn flows_backward() -> u16 {
    let x = 5;
    let y = x * 2;
    y
}

fn loops(n: u64) -> u64 {
    let mut i = 0;
    let first_square = loop {
        if i * i >= n {
            break i;
        }
        i += 1;
    };
    let mut count = 0;
    while count < n {
        count += 1;
    }
    let mut spins = 0;
    loop {
        spins += 1;
        if spins == 3 {
            break;
        }
    }
    first_square + count + spins
}

fn never(flag: bool) -> i32 {
    let x = if flag { 1 } else { return -1 };
    let y: i32 = if flag { unsafe { abort() } } else { 2 };
    let z = loop {
        if flag {
            break 3;
        }
    };
    x + y + z
}

fn diverging_block() -> u8 {
    return 1;
}

fn deferred_init(flag: bool) -> i32 {
    let x;
    if flag {
        x = 1;
    } else {
        x = 2;
    }
    x
}

fn operators(a: i8, b: u32, f: bool) -> bool {
    let shifted = a << b;
    let masked = b & 0xff;
    let flags = f ^ true | false & f;
    let min = -128i8;
    let unit = () == ();
    shifted < a && masked > 0 && flags || unit && min < 0 && !f
}

fn casts(x: i32, f: bool) -> u64 {
    (x as u64) + (f as u64) + (-1i64 as u64) + (300i32 as u8 as u64)
}
