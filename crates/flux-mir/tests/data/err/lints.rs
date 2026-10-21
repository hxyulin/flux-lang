// rustc's deny-by-default `arithmetic_overflow` and `unconditional_panic`
// lints: arithmetic in ordinary code whose operands are known.

const N: i32 = 1;

fn literals() {
    let _a = 255u8 + 1;
    let _b = 0u32 - 1;
    let _c = i32::MAX * 2;
    let _d = 1i64 << 64;
    let _e = 1u8 >> 8;
    let _f = N + i32::MAX;
    let _g = -i32::MIN;
    let _h = i8::MIN - 1;
}

fn locals() {
    let x: u8 = 255;
    let _a = x + 1;
    let y = i32::MIN;
    let _b = -y;
    let z = 40;
    let _c = 1i32 << z;
    let w: i16 = 200;
    let _d = w * w;
}

fn same_block() {
    let mut y: u8 = 250;
    y += 10;
    let mut z: i32 = 0;
    z = z + 1;
    z -= i32::MAX;
    z -= 3;
    let _r = (y, z);
}

fn panics() {
    let _a = 1 / 0;
    let _b = 7u64 % 0;
    let zero = 0;
    let _c = 10 / zero;
    let _d = i32::MIN / -1;
    let m = i8::MIN;
    let _e = m % -1;
}

fn main() {
    literals();
    locals();
    same_block();
    panics();
}
