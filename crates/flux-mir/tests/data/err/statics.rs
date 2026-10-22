struct Inner {
    v: i32,
}

struct S {
    a: i32,
    inner: Inner,
}

impl S {
    fn set(&mut self) {
        self.a = 0;
    }
    fn take(self) {}
}

static ST: S = S {
    a: 1,
    inner: Inner { v: 2 },
};
static mut MS: S = S {
    a: 1,
    inner: Inner { v: 2 },
};
static N: i32 = 1;

fn main() {
    let x = ST.inner;
    ST.a = 3;
    ST.inner.v = 4;
    N = 2;
    let r = &mut ST.a;
    let q = &mut N;
    ST.set();
    ST.take();
    unsafe {
        let y = MS.inner;
        let z = MS;
        MS.a = 5;
    }
}
