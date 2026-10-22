// Enums, their variants, and the patterns, `match`es and conditions
// that test them.

enum Shape {
    Circle(i32),
    Rect { w: i32, h: i32 },
    Empty,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Low = -1,
    Mid,
    High = 10,
}

pub enum Maybe<T> {
    Just(T),
    Nothing,
}

use Maybe::{Just, Nothing};

impl Shape {
    fn empty() -> Self {
        Self::Empty
    }

    fn sides(&self) -> i32 {
        match self {
            Self::Circle(_) => 0,
            Self::Rect { .. } => 4,
            Self::Empty => 0,
        }
    }
}

fn area(s: Shape) -> i32 {
    match s {
        Shape::Circle(r) if r > 0 => 3 * r * r,
        Shape::Circle(_) => 0,
        Shape::Rect { w, h: height } => w * height,
        Shape::Empty => 0,
    }
}

fn patterns(x: i32, pair: (bool, i32), m: Maybe<i32>) -> i32 {
    let a = match x {
        0 => 1,
        1 | 2 => 2,
        -5..=-1 => 3,
        3..10 => 4,
        10.. => 5,
        ..-5 => 6,
    };
    let b = match pair {
        (true, n @ 1..=9) => n,
        (false, ref n) => *n,
        (_, _) => 0,
    };
    let c = match m {
        Just(v) => v,
        Nothing => 0,
    };
    let d = match &Maybe::Just(1) {
        &Just(v) => v,
        &Nothing => 0,
    };
    a + b + c + d
}

fn conditions(m: Maybe<i32>, n: Maybe<i32>) -> i32 {
    if let Just(x) = m
        && x > 1
        && let Just(y) = n
    {
        return x + y;
    }
    let mut count = 0;
    while let Nothing = Maybe::<i32>::Nothing {
        count += 1;
        if count > 3 {
            break;
        }
    }
    let Just(z) = Maybe::Just(count) else {
        return 0;
    };
    z
}

fn main() {
    let _ = area(Shape::empty());
    let _ = Shape::Rect { w: 1, h: 2 }.sides();
    let _ = patterns(1, (true, 2), Maybe::Nothing);
    let _ = conditions(Just(1), Nothing);
    let _ = Level::Mid as i32;
    let _ = Level::Low == Level::High;
}
