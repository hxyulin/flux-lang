// Closures that capture what a reference points to, not the reference:
// `*r` for a method's receiver, a pattern, `len` and a reborrow, and
// nothing for `let _ = r`. Their bodies read the capture's field.

struct S {
    v: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.v
    }
}

fn by_mut(s: &mut S) {
    s.v += 1;
}

fn main() {
    let s = S { v: 7 };
    let r = &s;
    let method = || r.get();
    method();
    let pattern = || match r {
        S { v } => *v,
    };
    pattern();
    let ignored = || {
        let _ = r;
    };
    ignored();
    let a = [1, 2, 3];
    let slice: &[i32] = &a;
    let len = || slice.len();
    len();
    let mut t = S { v: 1 };
    let m = &mut t;
    let mut reborrow = || by_mut(m);
    reborrow();
}
