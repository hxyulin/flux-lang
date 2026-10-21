struct Unit;

struct U;

struct Empty {}

struct Point {
    x: i32,
    y: i32,
}

struct Pair(i32, bool);

struct Line {
    start: Point,
    end: Point,
    width: u8,
}

fn literals(x: i32) -> Line {
    let y = 2;
    let start = Point { x, y };
    let end = Point { x: 3, ..start };
    let _unit = Unit;
    let _braces = Unit {};
    let _empty = Empty {};
    let _pair = Pair(1, true);
    let _named = Pair { 0: 4, 1: false };
    Line {
        start,
        end,
        width: 1,
    }
}

fn access(line: Line, t: ((i32, u8), bool)) -> i32 {
    let a = line.start.x + line.end.y;
    let b = t.0.1;
    let c = t.0.0;
    let p = Pair(a + line.width as i32, t.1);
    if p.1 { a + p.0 + c } else { b as i32 }
}

fn patterns(line: Line, pair: Pair, triple: (i32, i32, i32)) -> i32 {
    let Line { start, end: Point { x, .. }, .. } = line;
    let Pair(n, _) = pair;
    let (first, ..) = triple;
    let (.., last) = triple;
    let (a, .., c) = triple;
    let Point { y: mut row, x: _ } = start;
    row += 1;
    let U = U;
    let Unit = Unit;
    x + n + first + last + a + c + row
}

fn params(Point { x, y }: Point, Pair(n, flag): Pair, (a, b): (i32, i32), Unit: Unit) -> i32 {
    if flag { x + y + n } else { a + b }
}

fn nested((Pair(a, ..), Point { x, .. }): (Pair, Point)) -> i32 {
    a + x
}

fn main() {
    let line = literals(1);
    let _ = access(line, ((1, 2), true));
    let _ = patterns(literals(2), Pair(3, false), (1, 2, 3));
    let _ = params(Point { x: 1, y: 2 }, Pair(3, true), (4, 5), Unit);
    let _ = nested((Pair(1, true), Point { x: 2, y: 3 }));
}
