//! `print!` and `println!` with format specs: fill, alignment, sign,
//! `#`, `0`, width and precision (written, from arguments, named and
//! captured), every integer type and radix, `bool`, `char` and `str`,
//! escapes in `{:?}`, positional, named and captured arguments, and
//! `{{`/`}}`. Values pass through functions so nothing is folded.

fn ints(a: i8, b: i16, c: i32, d: i64, e: i128, f: isize) {
    println!("{} {} {} {} {} {}", a, b, c, d, e, f);
    println!("{:x} {:x} {:x} {:x} {:x} {:x}", a, b, c, d, e, f);
    println!("{:X} {:o} {:b}", c, c, a);
    println!("{:#x} {:#X} {:#o} {:#b}", d, d, b, a);
    println!("[{:+}] [{:+}] [{:08}] [{:+08}] [{:<8}] [{:^8}] [{:>8}]", c, -c, c, c, c, c, c);
    println!("[{:#010x}] [{:#10x}] [{:<#10x}] [{:^#12b}] [{:010b}]", c, c, c, a, a);
    println!("[{:*^9}] [{:_<6}] [{:->7}] [{:é^7}]", d, b, a, f);
    println!("{:?} {:x?} {:X?} {:#x?} {:5?}|", c, c, c, c, a);
}

fn unsigned(a: u8, b: u16, c: u32, d: u64, e: u128, f: usize) {
    println!("{} {} {} {} {} {}", a, b, c, d, e, f);
    println!("{:x} {:o} {:b} {:X}", a, b, c, d);
    println!("{:#x} {:#b} {:08X} {:#o}", e, a, c, f);
    println!("[{:>5}] [{:<5}] [{:^5}] [{:05}]", a, a, a, a);
}

fn extremes() {
    println!("{} {}", i8::MIN, i8::MAX);
    println!("{} {}", i16::MIN, u16::MAX);
    println!("{} {}", i32::MIN, u32::MAX);
    println!("{} {}", i64::MIN, u64::MAX);
    println!("{} {}", i128::MIN, i128::MAX);
    println!("{}", u128::MAX);
    println!("{:x} {:b}", i128::MIN, i64::MIN);
    println!("{:#x}", u128::MAX);
}

fn strings(s: &str, t: &str) {
    println!("[{}] [{:8}] [{:<8}] [{:>8}] [{:^8}] [{:^9}]", s, s, s, s, s, s);
    println!("[{:.2}] [{:8.3}] [{:>8.3}] [{:.0}] [{:.10}]", s, s, s, s, s);
    println!("[{:-^12}] [{:=>3}]", t, t);
    println!("{:?} {:?}", s, t);
    println!("{:?}", "tab\there\nnew \"quoted\" \\ back 'single' \r \0 end");
    println!("{:?} {:?}", "\u{7f}\u{1b}", "caf\u{e9} \u{301}x \u{200b}");
    println!("{:10?}|{:<12?}|", "pad", "ignored");
}

fn chars(a: char, b: char) {
    println!("{} {} [{:3}] [{:<3}] [{:^5}] [{:>3}]", a, b, a, a, b, b);
    println!("{:?} {:?} {:?} {:?} {:?}", a, b, '\'', '"', '\n');
    println!("{:?} {:?} {:?}", '\t', '\\', '\u{0}');
    println!("{:?} {:?}", '\u{e9}', '\u{10ffff}');
}

fn bools(a: bool) {
    println!("{} {} [{:6}] [{:>6}] [{:^7}] {:?}", a, !a, a, !a, a, a);
}

fn arguments(x: i32, width: usize, precision: usize) {
    let name = "flux";
    println!("{0} {1} {0} {1:?}", x, name);
    println!("{a} {b} {a:>w$}", a = x, b = name, w = width);
    println!("{name} {x} {x:?} {name:?} {name:>width$}");
    println!("[{:1$}] [{:>1$}] [{:.*}] [{:.prec$}]", x, width, 2, "abcdef", "xyz", prec = precision);
    println!("[{:>width$.precision$}] [{:^width$}]", "truncated", x);
    println!("[{x:0width$}] [{x:+0width$}] [{0:0>5}] [{0:0<5}]", -x);
    println!("{{}} {{{}}} }}{{ {{x}}", x);
    println!("{}{}{}", 1, "", 3);
    println!();
    println!("{}", "");
}

fn exps(a: i32, b: u64, c: i128) {
    println!("{:e} {:E} {:e} {:e} {:e}", a, a, b, c, 0u8);
    println!("{:.2e} {:.0e} {:.5e} {:e} {:e}", a, a, a, 1250u32, 1350u32);
    println!("[{:12e}] [{:<12e}] [{:+e}] [{:012e}] [{:+012.3e}]", a, a, a, -a, a);
    println!("{:.1e} {:.1e} {:.0e} {:e}", 95u8, 9_950u32, 5u8, u128::MAX);
}

fn print_pieces(n: u32) {
    print!("a");
    print!("{}", n);
    print!("b\nc");
    print!("{:>4}\n", n);
    println!("{}", n);
}

fn main() {
    ints(-5, 300, 255, -1234567890123, -170141183460469231731687303715884105728, 42);
    ints(127, -32768, -1, 9223372036854775807, 12345678901234567890123, -7);
    unsigned(200, 65535, 4000000000, 18446744073709551615, 340282366920938463463374607431768211455, 0);
    extremes();
    strings("hello", "é😀");
    strings("", "x");
    chars('a', 'é');
    chars('😀', ' ');
    bools(true);
    arguments(42, 7, 3);
    arguments(-3, 2, 0);
    exps(123456, 1000, -170141183460469231731687303715884105728);
    exps(-7, 99999, 0);
    print_pieces(9);
}
