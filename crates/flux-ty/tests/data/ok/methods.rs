struct Point {
    x: i32,
    y: i32,
}

struct Line {
    from: Point,
    to: Point,
}

impl Point {
    fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    fn origin() -> Point {
        Point::new(0, 0)
    }

    fn sum(&self) -> i32 {
        self.x + self.y
    }

    fn shift(&mut self, d: i32) {
        self.x += d;
        self.y += d;
    }

    fn moved(mut self, d: i32) -> Self {
        self.shift(d);
        self
    }

    fn dot(self, other: &Point) -> i64 {
        (self.x * other.x + self.y * other.y) as i64
    }
}

impl Line {
    fn len_x(&self) -> i32 {
        let from = &self.from;
        let to = &self.to;
        to.x - from.x
    }
}

fn first(p: &Point) -> i32 {
    p.x
}

fn set(p: &mut i32, v: i32) {
    *p = v;
}

pub fn run(flag: bool) -> i64 {
    let mut p = Point::new(1, 2);
    let a = p.sum();
    p.shift(3);
    let shared = &p;
    let b = shared.sum() + Point::sum(shared) + first(shared) + (*shared).y;
    let b2 = (&p).sum() + Point::sum(&p);
    let unique = &mut p;
    unique.shift(1);
    Point::shift(unique, 2);
    unique.y = 4;
    let mut n = 0u8;
    let rn = &mut n;
    *rn += 1;
    let copy = *rn;
    let mut v = 0;
    set(&mut v, 9);
    let tmp = &Point::origin();
    let c = tmp.sum() + v;
    let mut line = Line {
        from: Point::new(0, 0),
        to: Point::new(5, 1),
    };
    let d = line.len_x();
    line.from.shift(1);
    let pick = if flag { &line.from } else { &line.to };
    let e = pick.x + d;
    let q = Point::origin().moved(2);
    let f = q.dot(&Point::new(1, 1));
    (a + b + b2 + c + e) as i64 + copy as i64 + f
}
