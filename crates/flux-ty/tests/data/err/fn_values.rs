fn double(x: i32) -> i32 {
    x * 2
}

fn byte(x: u8) -> u8 {
    x
}

unsafe fn danger() -> i32 {
    1
}

const fn calls_pointer(f: fn(i32) -> i32) -> i32 {
    f(1)
}

fn main() {
    let a: fn(i32) -> i32 = byte;
    let b: i32 = double;
    let c: fn() -> i32 = danger;
    let d = 5;
    d(1);
    let e = double;
    e(1, 2);
    let f: fn(i32) -> i32 = double;
    let g: u8 = f(1);
}
