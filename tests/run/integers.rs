//! Wrapping arithmetic, division, casts, comparisons and shifts on every
//! integer width. Operands arrive as parameters, so nothing is folded at
//! compile time.

unsafe extern "C" {
    fn putchar(c: i32) -> i32;
}

fn print_u64(n: u64) {
    if n >= 10 {
        print_u64(n / 10);
    }
    unsafe {
        putchar(48 + (n % 10) as i32);
    }
}

fn print_i64(n: i64) {
    if n < 0 {
        unsafe {
            putchar(45);
        }
        print_u64(!(n as u64) + 1);
    } else {
        print_u64(n as u64);
    }
}

fn line(n: i64) {
    print_i64(n);
    unsafe {
        putchar(10);
    }
}

fn add_u8(a: u8, b: u8) -> u8 {
    a + b
}

fn sub_i16(a: i16, b: i16) -> i16 {
    a - b
}

fn mul_i32(a: i32, b: i32) -> i32 {
    a * b
}

fn neg_i8(a: i8) -> i8 {
    -a
}

fn div_i32(a: i32, b: i32) -> i32 {
    a / b
}

fn rem_i32(a: i32, b: i32) -> i32 {
    a % b
}

fn div_u64(a: u64, b: u64) -> u64 {
    a / b
}

fn shl_u8(a: u8, b: u32) -> u8 {
    a << b
}

fn shr_i32(a: i32, b: u8) -> i32 {
    a >> b
}

fn shr_u32(a: u32, b: i64) -> u32 {
    a >> b
}

fn shl_i64(a: i64, b: i64) -> i64 {
    a << b
}

fn mul_i128(a: i128, b: i128) -> i128 {
    a * b
}

fn lt_i32(a: i32, b: i32) -> bool {
    a < b
}

fn gt_u32(a: u32, b: u32) -> bool {
    a > b
}

fn bits(a: bool, b: bool) -> i64 {
    ((a & b) as i64) * 1000 + ((a | b) as i64) * 100 + ((a ^ b) as i64) * 10 + (!a as i64)
}

fn main() {
    line(add_u8(200, 100) as i64);
    line(sub_i16(-32768, 1) as i64);
    line(mul_i32(65536, 65537) as i64);
    line(neg_i8(-128) as i64);
    line(div_i32(-7, 2) as i64);
    line(rem_i32(-7, 2) as i64);
    line(rem_i32(7, -2) as i64);
    line(div_u64(18446744073709551615, 3) as i64);
    line(-1i32 as u8 as i64);
    line(300i32 as u8 as i64);
    line(-1i8 as u32 as i64);
    line(255u8 as i8 as i64);
    line(true as i64);
    line(lt_i32(-1, 1) as i64);
    line(gt_u32(4294967295, 1) as i64);
    line(shl_u8(1, 9) as i64);
    line(shr_i32(-16, 2) as i64);
    line(shr_u32(2147483648, 31) as i64);
    line(shl_i64(1, 70));
    let big = mul_i128(1_000_000_000_000_000_000, 1_000_000_000_000_000_000);
    line((big / 1_000_000_000_000_000_000) as i64);
    line((big % 1_000_000_007) as i64);
    line(bits(true, false));
    line(bits(true, true));
}
