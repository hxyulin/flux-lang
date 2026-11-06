// Values of several leaves returned in registers or through memory, by
// direct calls, function pointers, trait objects and closures.

#[derive(Debug, Clone, Copy, PartialEq)]
struct Mixed {
    a: u8,
    b: f64,
    c: i16,
    d: f32,
}

trait Split {
    fn split(&self, at: usize) -> (&str, &str);
    fn bounds(&self) -> Option<(u32, i64)>;
}

struct Text(&'static str);

impl Split for Text {
    fn split(&self, at: usize) -> (&str, &str) {
        self.0.split_at(at)
    }
    fn bounds(&self) -> Option<(u32, i64)> {
        if self.0.is_empty() {
            None
        } else {
            Some((self.0.len() as u32, -(self.0.len() as i64)))
        }
    }
}

#[inline(never)]
fn slice_of(s: &str, n: usize) -> &str {
    &s[n..]
}

#[inline(never)]
fn position(v: &[u32], x: u32) -> Option<usize> {
    v.iter().position(|&y| y == x)
}

#[inline(never)]
fn narrow_pair(a: u8, b: u8) -> (u8, u8) {
    (b, a)
}

#[inline(never)]
fn wide_and_narrow(a: u64, b: bool) -> (bool, u64) {
    (!b, a.wrapping_mul(3))
}

#[inline(never)]
fn floats(x: f64) -> (f64, f64) {
    (x * 0.5, x + 0.25)
}

#[inline(never)]
fn singles(x: f32) -> (f32, f32, f32, f32) {
    (x, x * 2.0, x * 3.0, x * 4.0)
}

#[inline(never)]
fn mixed(n: u8) -> Mixed {
    Mixed {
        a: n,
        b: n as f64 / 4.0,
        c: -(n as i16),
        d: n as f32 * 1.5,
    }
}

#[inline(never)]
fn seven(n: u64) -> (u64, u64, u64, u64, u64, u64, u64) {
    (n, n + 1, n + 2, n + 3, n + 4, n + 5, n + 6)
}

#[inline(never)]
fn eight(n: u64) -> (u64, u64, u64, u64, u64, u64, u64, u64) {
    (n, n + 1, n + 2, n + 3, n + 4, n + 5, n + 6, n + 7)
}

#[inline(never)]
fn wide(n: u64) -> (u128, u8) {
    ((n as u128) << 70 | 5, n as u8)
}

#[inline(never)]
fn result_of(n: i32) -> Result<u64, &'static str> {
    if n < 0 { Err("negative") } else { Ok(n as u64 * 10) }
}

fn recurse(n: u32) -> (u32, u64) {
    if n == 0 {
        (0, 1)
    } else {
        let (depth, product) = recurse(n - 1);
        (depth + 1, product * n as u64)
    }
}

fn apply<F: Fn(u64) -> (u64, u64)>(f: F, n: u64) -> (u64, u64) {
    f(n)
}

fn main() {
    println!("{}", slice_of("register", 3));
    println!("{:?} {:?}", position(&[4, 5, 6], 6), position(&[4, 5, 6], 7));
    println!("{:?}", narrow_pair(1, 2));
    println!("{:?}", wide_and_narrow(7, false));
    println!("{:?}", floats(3.0));
    println!("{:?}", singles(1.5));
    println!("{:?}", mixed(9));
    println!("{:?}", seven(10));
    println!("{:?}", eight(20));
    println!("{:?}", wide(3));
    println!("{:?} {:?}", result_of(4), result_of(-1));
    println!("{:?}", recurse(10));

    // Through function pointers, whose targets use the C convention.
    let pointers: [fn(&str, usize) -> &str; 1] = [slice_of];
    println!("{}", pointers[0]("pointer", 2));
    let p: fn(u8, u8) -> (u8, u8) = narrow_pair;
    let q: fn(u64, bool) -> (bool, u64) = wide_and_narrow;
    let r: fn(f64) -> (f64, f64) = floats;
    let s: fn(u64) -> (u64, u64, u64, u64, u64, u64, u64) = seven;
    let t: fn(f32) -> (f32, f32, f32, f32) = singles;
    let u: fn(u8) -> Mixed = mixed;
    let v: fn(u32) -> (u32, u64) = recurse;
    println!("{:?} {:?} {:?}", p(3, 4), q(8, true), r(1.0));
    println!("{:?} {:?}", s(1), t(0.5));
    println!("{:?} {:?}", u(2), v(5));

    // Through trait objects.
    let objects: [&dyn Split; 2] = [&Text("object"), &Text("")];
    for object in objects {
        if let Some((len, neg)) = object.bounds() {
            println!("{:?} {len} {neg}", object.split(2));
        } else {
            println!("empty");
        }
    }

    // Through closures, by value and as a pointer.
    let offset = 100;
    println!("{:?}", apply(|n| (n + offset, n * offset), 3));
    let closure: fn(u64) -> (u64, u64) = |n| (n, n * n);
    println!("{:?}", apply(closure, 12));
}
