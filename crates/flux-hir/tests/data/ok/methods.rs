struct Counter {
    n: i32,
    step: i32,
}

impl Counter {
    fn new(step: i32) -> Self {
        Self { n: 0, step }
    }

    fn zero() -> Counter {
        Self::new(0)
    }

    fn get(&self) -> i32 {
        self.n
    }

    fn bump(&mut self) {
        self.n += self.step;
    }
}

impl Counter {
    fn reset(mut self) -> Self {
        self.n = 0;
        self
    }

    fn into_n(self) -> i32 {
        self.n
    }

    fn total(&self, other: &Self) -> i32 {
        self.get() + other.get()
    }

    fn copy_from(&mut self, other: &Counter) {
        self.n = other.n;
        self.step = (*other).step;
    }

    fn twice(&mut self) {
        self.bump();
        Self::bump(self);
        Counter::bump(self);
    }
}

struct Wrapper {
    inner: Counter,
}

impl Wrapper {
    fn make() -> Self {
        let inner = Counter::new(2);
        Wrapper { inner }
    }

    fn peek(&self) -> i32 {
        self.inner.get()
    }

    fn poke(&mut self) {
        self.inner.bump();
    }
}

fn read(r: &i32) -> i32 {
    *r
}

fn write(r: &mut i32, v: i32) {
    *r = v;
}

pub fn main_like() -> i32 {
    let mut c = Counter::new(3);
    c.bump();
    let r = &c;
    let a = r.get() + Counter::get(r) + Counter::get(&c);
    let m = &mut c;
    m.bump();
    (*m).bump();
    m.n += 1;
    let mut x = 5;
    let rx = &mut x;
    *rx += 1;
    write(&mut x, 7);
    let b = read(&x) + read(&1);
    let mut z = Counter::zero();
    z.copy_from(&c);
    z.twice();
    let d = z.total(&c);
    let mut w = Wrapper::make();
    w.poke();
    let e = w.peek() + (&w).peek() + w.inner.n;
    let f = c.reset().into_n();
    a + b + d + e + f
}
