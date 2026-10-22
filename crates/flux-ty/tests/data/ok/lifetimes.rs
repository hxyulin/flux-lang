// Lifetime parameters, bounds and arguments in signatures.

struct S {
    a: i32,
}

impl S {
    fn get<'a>(&'a self) -> i32 {
        self.a
    }

    fn add<'a, 'b>(&'a mut self, other: &'b S) {
        self.a += other.a;
    }
}

fn sum<'a, 'b: 'a, T: 'a>(x: &'a i32, y: &'b i32, _t: &'a T) -> i32
where
    'a: 'a,
    T: Copy + 'b,
{
    *x + *y
}

fn elided(x: &'_ i32, y: &'static i32) -> i32 {
    let z: &'_ i32 = x;
    *z + *y
}

fn main() {
    let mut s = S { a: 1 };
    let t = S { a: 2 };
    s.add(&t);
    let x = 3;
    let n = sum(&x, &x, &x) + elided(&x, &5) + s.get();
    let _ = n;
}
