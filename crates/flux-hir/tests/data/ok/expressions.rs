#![allow(unused)]

fn literals() -> u64 {
    let a = 1_000;
    let b = 0xff_u8;
    let c: i64 = -0b1010;
    let t = true;
    let unit = ();
    a as u64 + b as u64 + 0o17
}

fn operators(x: i32, y: i32) -> bool {
    let mut z = x + y * 2 - (x - y) / 3 % 4;
    z <<= 1;
    z |= x & y ^ !x;
    z = -z >> 1;
    x < y && y <= z || !(x == y) && x != z
}

fn control(n: u32) -> u32 {
    let mut i = 0;
    let found = loop {
        if i * i > n {
            break i;
        } else if i == 100 {
            return 0;
        }
        i += 1;
    };
    let mut sum = 0;
    while sum < n {
        sum += 1;
        if sum % 2 == 0 {
            continue;
        }
    }
    found + sum
}

fn shadowing(x: i32) -> i32 {
    let x = x + 1;
    let y = {
        let x = x * 2;
        x
    };
    let _ = y;
    x + y
}

fn diverges() -> i32 {
    return 1;
}

fn question_marks(a: Option<i32>, b: Result<i32, u8>) -> Option<i32> {
    let x = a? + 1;
    Some(x + b.ok()?)
}
