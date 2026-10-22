// Function items as values, their coercion to function pointers, and
// calls through both.

#[derive(Clone, Copy)]
struct Pair {
    a: i64,
    b: i64,
}

struct Op {
    f: fn(i32) -> i32,
}

impl Op {
    fn run(&self, x: i32) -> i32 {
        (self.f)(x)
    }
    fn make(x: i32) -> i32 {
        x + 1
    }
}

trait Shape {
    fn area(&self) -> i32;
}

struct Square(i32);

impl Shape for Square {
    fn area(&self) -> i32 {
        self.0 * self.0
    }
}

fn double(x: i32) -> i32 {
    x * 2
}

fn negate(x: i32) -> i32 {
    -x
}

fn id<T>(x: T) -> T {
    x
}

fn swap(p: Pair) -> Pair {
    Pair { a: p.b, b: p.a }
}

unsafe fn danger(x: i32) -> i32 {
    x
}

fn apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn twice<T: Copy>(f: fn(T) -> T, x: T) -> T {
    f(f(x))
}

fn pick(flag: bool) -> fn(i32) -> i32 {
    if flag { double } else { negate }
}

fn main() {
    let d = double;
    let a = d(5);
    let p: fn(i32) -> i32 = negate;
    let b = p(a) + apply(double, 2) + twice(negate, 3);
    let c = twice::<u8>(id, 9);
    let e = pick(true)(1);
    let op = Op { f: Op::make };
    let r = op.run(1);
    let s = (swap as fn(Pair) -> Pair)(Pair { a: 1, b: 2 });
    let area: fn(&Square) -> i32 = <Square as Shape>::area;
    let t = area(&Square(2));
    let u: unsafe fn(i32) -> i32 = danger;
    let v = unsafe { u(1) + p(2) };
    let w = match a {
        10 => double,
        _ => id,
    };
    let x = w(3);
}
