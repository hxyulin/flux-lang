//! Enums: generic ones and their methods, values with destructors moved
//! out of variants in matches (dropped as rustc drops them), derived
//! traits, discriminants and casts, and guards that fall through to the
//! next alternative of an or-pattern.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(n: i64) {
    unsafe {
        printf(c"%lld\n".as_ptr(), n);
    }
}

struct Noisy(i64);

impl Drop for Noisy {
    fn drop(&mut self) {
        show(1000 + self.0);
    }
}

enum Maybe<T> {
    Just(T),
    Nothing,
}

impl<T> Maybe<T> {
    fn is_just(&self) -> bool {
        match self {
            Maybe::Just(_) => true,
            Maybe::Nothing => false,
        }
    }

    fn unwrap_or(self, default: T) -> T {
        match self {
            Maybe::Just(x) => x,
            Maybe::Nothing => default,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Green,
    Blue = 10,
    Other,
}

#[derive(Clone, PartialEq)]
enum Tree {
    Leaf(i64),
    Pair { left: i64, right: i64 },
}

fn pick(x: (i64, i64)) -> i64 {
    match x {
        (a, 0) | (0, a) if a > 10 => a,
        (a, b) if a == b => 1,
        _ => 2,
    }
}

fn main() {
    let a = Maybe::Just(Noisy(1));
    let b: Maybe<Noisy> = Maybe::Nothing;
    show(a.is_just() as i64);
    show(b.is_just() as i64);
    let c = a.unwrap_or(Noisy(2));
    show(c.0);
    {
        let d = Maybe::Just(Noisy(3));
        if let Maybe::Just(n) = d {
            show(n.0);
        }
        show(50);
    }
    {
        let e = Maybe::Just(Noisy(4));
        let f = Maybe::Just(Noisy(5));
        match e {
            Maybe::Just(ref n) => show(n.0),
            Maybe::Nothing => show(0),
        }
        if true {
            let Maybe::Just(_g) = f else { return; };
            show(60);
        }
        show(61);
    }
    show(Color::Red as i64);
    show(Color::Blue as i64);
    show(Color::Other as i64);
    let col = Color::Green;
    let col2 = col;
    show((col == col2) as i64);
    show((col == Color::Red) as i64);
    let t = Tree::Pair { left: 1, right: 2 };
    let u = t.clone();
    show((t == u) as i64);
    show((Tree::Leaf(3) == Tree::Leaf(4)) as i64);
    show((Tree::Leaf(3) == Tree::Pair { left: 3, right: 3 }) as i64);
    show(pick((0, 20)));
    show(pick((20, 0)));
    show(pick((5, 0)));
    show(pick((7, 7)));
    show(70);
}
