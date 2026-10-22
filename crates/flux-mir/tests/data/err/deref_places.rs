use core::ops::{Deref, DerefMut, Index};

struct S;

struct P {
    s: S,
    n: i32,
}

impl P {
    fn bump(&mut self) {
        self.n += 1;
    }
}

struct W {
    p: P,
}

impl Deref for W {
    type Target = P;
    fn deref(&self) -> &P {
        &self.p
    }
}

struct M {
    p: P,
}

impl Deref for M {
    type Target = P;
    fn deref(&self) -> &P {
        &self.p
    }
}

impl DerefMut for M {
    fn deref_mut(&mut self) -> &mut P {
        &mut self.p
    }
}

struct I {
    a: [S; 2],
}

impl Index<usize> for I {
    type Output = S;
    fn index(&self, i: usize) -> &S {
        &self.a[i]
    }
}

fn takes_mut(_: &mut P) {}

fn main() {
    let mut w = W { p: P { s: S, n: 1 } };
    w.n = 2;
    let _r = &mut w.n;
    w.bump();
    takes_mut(&mut w);
    let _s = w.s;
    let _p = *w;
    let mut m = M { p: P { s: S, n: 1 } };
    m.n = 3;
    m.bump();
    let _t = m.s;
    let i = I { a: [S, S] };
    let _x = i[0];
}
