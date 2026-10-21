unsafe extern "C" {
    fn abs(x: i32) -> i32;
}

fn mismatches(flag: bool) -> i32 {
    let a: u8 = flag;
    let b = if flag { 1 } else { false };
    if 1 {}
    let c: bool = 5;
    true
}

fn operators(x: u32, f: bool) {
    let a = -x;
    let b = f + f;
    let c = !();
    let d = 1u8 + 1i32;
}

fn literals() {
    let a = 256u8;
    let b: i8 = 128;
    let c = -129i8;
    let d = -128i8;
}

fn calls() {
    abs(1);
    let x = calls(1);
    let y = unsafe { abs() };
    let z = 5(1);
}

fn mutability(p: i32) {
    let x = 1;
    x = 2;
    p += 1;
    let y;
    y = 1;
    y += 1;
    3 = 4;
}

fn loops() {
    while true {
        break 5;
    }
    let v: u8 = loop {
        break true;
    };
}

fn returns() -> u8 {
    if false {
        return;
    }
    return 300;
}

fn types(x: str, y: (i32, i32)) -> Vec {
    let a = 1 as bool;
    let b = () as i32;
    let c;
    fn_value;
    loop {}
}

fn fn_value() {}
