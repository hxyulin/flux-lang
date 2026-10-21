struct Unit;

struct Point {
    x: i32,
    y: i32,
}

struct Pair(u8, bool);

struct Mixed {
    small: u8,
    big: i64,
    flag: bool,
    point: Point,
    pair: Pair,
}

fn make(n: i64) -> Mixed {
    let point = Point { x: 1, y: 2 };
    let pair = Pair(3, true);
    let small = 4;
    Mixed {
        small,
        big: n,
        flag: false,
        point,
        pair,
    }
}

fn fields(m: Mixed) -> i64 {
    let a = m.small;
    let b = m.point.x;
    let c = m.pair.0;
    let d = m.pair.1;
    let e = (m.big, (a, d));
    let f = e.1.0;
    let g = e.1;
    if g.1 { m.big + b as i64 + c as i64 + f as i64 } else { 0 }
}

fn update(base: Mixed) -> Mixed {
    let tail = 7;
    let m = Mixed {
        big: tail,
        point: Point { y: 5, ..base.point },
        ..base
    };
    let unit = Unit;
    let unit2 = Unit {};
    let Unit = unit;
    let Unit {} = unit2;
    m
}

fn patterns(m: Mixed, t: (u16, (i8, bool), u32)) -> u32 {
    let Mixed { small, point: Point { x, .. }, pair: Pair(p, _), .. } = m;
    let (first, (second, flag), ..) = t;
    let (.., last) = t;
    let mut acc = 0;
    let on = m.flag;
    if flag == on {
        acc = small as u32 + p as u32 + x as u32 + first as u32 + second as u32;
    }
    acc + last
}

fn tuples() -> (i32, (bool, u64)) {
    let inner = (true, 9);
    let unit = ();
    let one = (5u8,);
    let n = one.0;
    let _ = unit;
    (n as i32, inner)
}

fn mutate(mut p: Point, mut t: (i32, i32)) -> i32 {
    p.x = 3;
    p.y += p.x;
    t.0 = p.y;
    t.1 *= 2;
    t.0 + t.1
}

fn main() {
    let m = make(10);
    let _ = fields(make(1));
    let _ = patterns(update(m), (1, (2, true), 3));
    let _ = tuples();
    let _ = mutate(Point { x: 0, y: 0 }, (1, 2));
}
