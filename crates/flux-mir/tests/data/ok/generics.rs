// Moves and borrows of generic values: a `T` is never `Copy`.

pub struct Pair<T> {
    a: T,
    b: T,
}

pub struct Wrap<T>(T);

impl<T> Pair<T> {
    fn new(a: T, b: T) -> Self {
        Pair { a, b }
    }

    fn into_a(self) -> T {
        self.a
    }

    fn set_a(&mut self, a: T) {
        self.a = a;
    }

    fn replace_a(&mut self, other: Pair<T>) -> T {
        let Pair { a, b } = other;
        self.set_a(a);
        b
    }

    fn rebuild(self) -> Self {
        let Pair { a, b } = self;
        Pair { a: b, b: a }
    }
}

impl Pair<i32> {
    fn total(&self) -> i32 {
        self.a + self.b
    }

    fn bump(&mut self, by: i32) {
        self.a += by;
    }
}

fn id<T>(x: T) -> T {
    x
}

// Each value moves once, on every path.
fn moves<T>(x: T, y: T, flag: bool) -> Wrap<T> {
    let z = id(x);
    let w = if flag { y } else { id(z) };
    Wrap(w)
}

fn move_then_reassign<T>(mut x: T, y: T) -> T {
    let _first = x;
    x = y;
    x
}

fn move_in_loop_once<T>(x: T) -> Wrap<T> {
    loop {
        break Wrap(x);
    }
}

// Moving one field leaves the other usable.
fn partial<T>(p: Pair<T>) -> Pair<T> {
    let a = p.a;
    Pair { a: p.b, b: a }
}

fn nested_partial<T>(p: Pair<Pair<T>>) -> T {
    let inner = p.a;
    let _ = p.b.a;
    inner.into_a()
}

// Borrows of generic struct fields.
fn borrows<T>(mut p: Pair<T>, x: T, y: T) -> Pair<T> {
    let r = &p.a;
    let s = &p.b;
    let _r2 = r;
    let _s2 = s;
    // Disjoint fields: a shared and a unique borrow at once.
    let m = &mut p.b;
    let t = &p.a;
    *m = x;
    let _t2 = t;
    let n = &mut p.a;
    *n = y;
    p
}

// Methods through `&self` and `&mut self`, and two-phase borrows.
fn methods(x: i32) -> i32 {
    let mut p = Pair::new(x, 2);
    p.bump(p.total());
    p.bump(1);
    let q = &mut p;
    q.bump(q.total());
    p.set_a(p.a + 1);
    let _ = (&p).total();
    p.rebuild().total()
}

fn generic_methods<T>(mut p: Pair<T>, x: T, y: T, z: Pair<T>) -> T {
    p.set_a(x);
    let r = &mut p;
    r.set_a(y);
    let _b = r.replace_a(z);
    p.into_a()
}

fn main() {
    let _ = methods(1);
    let p = Pair::new(1u8, 2);
    let _ = partial(p).rebuild().into_a();
}
