//! Enums and structs in memory, laid out as rustc lays them out.
unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}
fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Shape {
    Circle(u8),
    Rect(u16, u32),
    Empty,
    Tri(bool, char),
}

#[derive(Clone, Copy)]
enum Small {
    A,
    B,
    C = 200,
}

#[derive(Clone, Copy)]
struct Packed {
    a: u8,
    b: u64,
    c: u16,
    d: Option<bool>,
}

static TABLE: [Option<Shape>; 4] = [Some(Shape::Circle(3)), None, Some(Shape::Tri(true, 'z')), Some(Shape::Empty)];
const REFS: [Option<&'static u32>; 2] = [None, Some(&77)];

fn area(s: &Shape) -> i64 {
    match *s {
        Shape::Circle(r) => 3 * r as i64 * r as i64,
        Shape::Rect(w, h) => w as i64 * h as i64,
        Shape::Empty => 0,
        Shape::Tri(b, c) => b as i64 + c as i64,
    }
}

fn opt(o: Option<&u32>) -> i64 {
    match o {
        Some(v) => *v as i64,
        None => -1,
    }
}

fn twice(arr: [Option<Shape>; 3]) -> [Option<Shape>; 3] {
    [arr[2], arr[1], arr[0]]
}

fn main() {
    let x = 5u32;
    let y = 9u32;
    let mut refs = [Some(&x), None, Some(&y)];
    show(opt(refs[0]) + opt(refs[1]) + opt(refs[2]));
    refs[1] = Some(&y);
    refs[0] = None;
    show(opt(refs[0]) * 100 + opt(refs[1]));
    let mut shapes = [Shape::Circle(2), Shape::Rect(3, 70000), Shape::Empty, Shape::Tri(false, 'a')];
    let mut total = 0;
    for s in &shapes {
        total += area(s);
    }
    show(total);
    shapes[2] = Shape::Rect(1, 2);
    let r = &mut shapes[0];
    *r = Shape::Tri(true, 'b');
    if let Shape::Rect(w, h) = &mut shapes[1] {
        *w += 1;
        *h += 1;
    }
    total = 0;
    for s in shapes.iter() {
        total += area(s);
    }
    show(total);
    let mut nested: [Option<Option<bool>>; 4] = [None, Some(None), Some(Some(false)), Some(Some(true))];
    nested.swap(0, 3);
    for n in nested {
        show(match n {
            None => 0,
            Some(None) => 1,
            Some(Some(false)) => 2,
            Some(Some(true)) => 3,
        });
    }
    let small = [Small::A, Small::C, Small::B];
    for s in small {
        show(s as i64);
    }
    let mut p = [Packed { a: 1, b: 2, c: 3, d: None }, Packed { a: 4, b: 5, c: 6, d: Some(true) }];
    p[0].d = Some(false);
    let q = &mut p[1];
    q.b += 100;
    for x in p {
        show(x.a as i64 + x.b as i64 + x.c as i64 + match x.d { None => 1000, Some(true) => 2000, Some(false) => 3000 });
    }
    let t = twice([Some(Shape::Empty), None, Some(Shape::Circle(4))]);
    for s in t {
        show(match s { Some(s) => area(&s), None => -7 });
    }
    for s in TABLE {
        show(match s { Some(s) => area(&s), None => -9 });
    }
    show(opt(REFS[0]) + opt(REFS[1]));
    let results: [Result<u8, &str>; 3] = [Ok(4), Err("abc"), Ok(250)];
    for r in &results {
        show(match r { Ok(v) => *v as i64, Err(e) => e.len() as i64 * 10 });
    }
    let mut chars = [Some('x'), None];
    chars[1] = Some('y');
    show(chars[0].unwrap() as i64 + chars[1].unwrap() as i64);
    let pair: (Option<&u32>, Option<char>) = (Some(&x), None);
    let boxed = [pair, (None, Some('q'))];
    show(opt(boxed[0].0) + boxed[1].1.unwrap() as i64);
}
