// Functions with `#[inline]`, `#[inline(always)]`, `#[inline(never)]` and
// `#[cold]`: called directly, through function pointers and trait
// objects, recursively, with `#[track_caller]`, and on a cold panic path.

use std::hint::black_box;

#[inline]
fn hinted(x: u64) -> u64 {
    x.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (x >> 7)
}

#[inline(always)]
fn always(x: u64, y: u64) -> u64 {
    if x > y { x - y } else { y - x }
}

#[inline(never)]
fn never(values: &[u64]) -> u64 {
    values.iter().fold(0, |a, &v| a ^ hinted(v))
}

#[cold]
#[inline(never)]
fn report(code: u64) -> u64 {
    println!("cold path {code}");
    code + 1
}

#[inline(always)]
fn countdown(n: u32) -> u32 {
    if n == 0 { 0 } else { 1 + countdown(n - 1) }
}

#[inline]
#[track_caller]
fn where_called() -> u32 {
    std::panic::Location::caller().line()
}

trait Shape {
    fn area(&self) -> u64;
}

struct Square(u64);

impl Shape for Square {
    #[inline(always)]
    fn area(&self) -> u64 {
        self.0 * self.0
    }
}

struct Tiles<T>(T);

impl<T: Copy + Into<u64>> Tiles<T> {
    #[inline]
    fn total(&self, n: u64) -> u64 {
        self.0.into() * n
    }
}

#[cold]
fn fail(message: &str) -> ! {
    panic!("{message}")
}

fn main() {
    let values: Vec<u64> = (1..=20).collect();
    let mut sum = 0u64;
    for &v in &values {
        sum = sum.wrapping_add(hinted(black_box(v)));
        sum = sum.wrapping_add(always(black_box(v), 10));
        if v % 7 == 0 {
            sum = sum.wrapping_add(report(v));
        }
    }
    println!("{sum} {}", never(&values));

    let pointers: [fn(u64) -> u64; 2] = [hinted, |x| always(x, 3)];
    let through: Vec<u64> = pointers.iter().map(|f| f(black_box(5))).collect();
    println!("{through:?} {}", pointers[0](9) == hinted(9));

    println!("{} {}", countdown(black_box(12)), where_called());

    let shapes: Vec<Box<dyn Shape>> = vec![Box::new(Square(3)), Box::new(Square(4))];
    let area: u64 = shapes.iter().map(|s| s.area()).sum();
    println!("{area} {} {}", Square(5).area(), Tiles(7u8).total(black_box(6)));

    if black_box(sum) == 0 {
        fail("unreachable");
    }
    std::process::exit((sum % 200) as i32);
}
