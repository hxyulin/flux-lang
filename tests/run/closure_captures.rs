//! Closures that capture places behind references: what a captured
//! `&T`, `&mut T`, `&[T]`, or a field, tuple or array element of one,
//! points to, used through method receivers (`&self`, `&mut self`,
//! `self` on `Copy`, trait and generic methods, `len`), reborrows passed
//! to functions, operators and comparisons, indexing, patterns that
//! match through the reference, `let _`, nested closures and iterator
//! adapters, each with and without `move`. A non-`move` closure captures
//! `*r` for `r.get()`, not `r`.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn show(label: i32, value: i32) {
    unsafe {
        printf(c"%d: %d\n".as_ptr(), label, value);
    }
}

#[derive(Clone, Copy, PartialEq)]
struct S {
    v: i32,
}

impl S {
    fn get(&self) -> i32 {
        self.v
    }

    fn bump(&mut self) -> i32 {
        self.v += 1;
        self.v
    }

    fn take(self) -> i32 {
        self.v * 10
    }

    fn pick<T: Copy>(&self, t: T) -> T {
        t
    }
}

impl core::ops::Add for S {
    type Output = S;

    fn add(self, other: S) -> S {
        S {
            v: self.v + other.v,
        }
    }
}

trait Area {
    fn area(&self) -> i32;
    fn scale(&mut self, k: i32);
}

impl Area for S {
    fn area(&self) -> i32 {
        self.v * self.v
    }

    fn scale(&mut self, k: i32) {
        self.v *= k;
    }
}

struct Outer {
    inner: S,
    n: i32,
}

enum E {
    A(i32),
    B { x: i32 },
}

fn by_ref(s: &S) -> i32 {
    s.v
}

fn by_mut(s: &mut S) {
    s.v += 5;
}

fn total<T: Area>(items: &[T]) -> i32 {
    let f = |i: usize| items[i].area();
    let mut sum = 0;
    for i in 0..items.len() {
        sum += f(i);
    }
    sum
}

fn area_of<T: Area>(t: &T) -> i32 {
    let f = || t.area();
    f()
}

fn scale_twice<T: Area>(t: &mut T) {
    let mut f = |k| t.scale(k);
    f(2);
    f(3);
}

fn call_plus_one<F: Fn(i32) -> i32>(f: &F, x: i32) -> i32 {
    let g = || f(x) + 1;
    g()
}

struct Counter {
    n: i32,
    step: i32,
}

impl Counter {
    fn get(&self) -> i32 {
        self.n
    }

    fn sum_with(&self, k: i32) -> i32 {
        let f = |x: i32| self.get() + x + self.step;
        f(k)
    }

    fn bump_twice(&mut self) {
        let mut f = || self.n += self.step;
        f();
        f();
    }

    fn bump_and_get(&mut self) -> i32 {
        let mut f = || {
            self.bump_twice();
            self.get()
        };
        f()
    }

    fn mapped(&self) -> i32 {
        (0..3).map(|i| i * self.step + self.get()).sum()
    }
}

struct Sieve {
    composite: [bool; 20],
}

impl Sieve {
    fn new() -> Sieve {
        let mut composite = [false; 20];
        composite[0] = true;
        composite[1] = true;
        for i in 2..20 {
            let mut j = i * 2;
            while j < 20 {
                composite[j] = true;
                j += i;
            }
        }
        Sieve { composite }
    }

    fn is_prime(&self, n: usize) -> bool {
        n < 20 && !self.composite[n]
    }
}

struct Table {
    values: [i32; 4],
}

impl Table {
    fn lookup(&self, i: usize) -> i32 {
        self.values[i]
    }

    fn has(&self, x: i32) -> bool {
        let values: &[i32] = &self.values;
        values.iter().any(|&v| v == x)
    }
}

fn receivers() {
    let mut s = S { v: 7 };
    {
        let r = &s;
        let f = || r.get();
        show(1, f());
        let f = move || r.get();
        show(2, f());
        let f = || r.area();
        show(3, f());
        let f = || r.pick(4);
        show(4, f());
        let f = || r.take();
        show(5, f());
        let f = move || r.take();
        show(6, f());
        let f = || r.v + r.get();
        show(7, f());
        let rr = &r;
        let f = || rr.get() + rr.v;
        show(8, f());
        show(9, area_of(r));
    }
    {
        let m = &mut s;
        let mut f = || m.bump();
        show(10, f());
        show(11, f());
        let mut f = || m.get();
        show(12, f());
        let mut f = move || m.bump();
        show(13, f());
    }
    show(14, s.v);
    scale_twice(&mut s);
    show(15, s.v);
    show(16, total(&[S { v: 1 }, S { v: 2 }]));
}

fn reborrows() {
    let mut s = S { v: 3 };
    let r = &s;
    let f = || by_ref(r);
    show(20, f());
    let f = move || by_ref(r);
    show(21, f());
    let m = &mut s;
    let mut f = || by_mut(m);
    f();
    f();
    let mut f = move || by_mut(m);
    f();
    show(22, s.v);
}

fn fields_tuples_arrays() {
    let o = Outer {
        inner: S { v: 3 },
        n: 4,
    };
    let ro = &o;
    let f = || ro.inner.get() + ro.n;
    show(30, f());
    let f = move || ro.inner.get() + ro.n;
    show(31, f());
    let mut o2 = Outer {
        inner: S { v: 3 },
        n: 4,
    };
    let mo = &mut o2;
    let mut f = || mo.inner.bump() + mo.n;
    show(32, f());
    show(33, o2.inner.v);

    let t = (S { v: 1 }, S { v: 2 });
    let rt = &t;
    let f = || rt.0.get() + rt.1.get();
    show(34, f());
    let f = || t.0.get();
    show(35, f());
    let p = S { v: 3 };
    let pair = (&p, 4);
    let f = || pair.0.area() + pair.1;
    show(36, f());
    let rpair = &pair;
    let f = move || rpair.0.area() + rpair.1;
    show(37, f());
    let refs = [&p, &p];
    let f = || refs[0].area() + refs[1].v;
    show(38, f());
    let rrefs = &refs;
    let f = || rrefs[1].get();
    show(39, f());
}

fn slices() {
    let a = [1, 2, 3, 4];
    let sl: &[i32] = &a;
    let f = || sl.len() as i32;
    show(40, f());
    let f = move || sl.len() as i32;
    show(41, f());
    let f = || sl[2] + sl.is_empty() as i32;
    show(42, f());
    let ra = &a;
    let f = || ra.len() as i32 + ra[1];
    show(43, f());
    let f = || match sl.first() {
        Some(x) => *x,
        None => -1,
    };
    show(44, f());
    let f = || sl.iter().count() as i32;
    show(45, f());
    // `len` borrows the array, so it may be used after.
    let structs = [S { v: 1 }, S { v: 2 }];
    let f = || structs.len() as i32;
    show(46, f());
    show(47, structs[1].v);

    let mut b = [5, 6, 7];
    let ms: &mut [i32] = &mut b;
    let mut f = || {
        ms[0] += 1;
        ms.len() as i32 + ms[0]
    };
    show(48, f());
    let g = || ms.len() as i32;
    show(49, g());
    ms[1] = 0;
    let mut h = move |i: usize| {
        ms[i] *= 10;
        ms[i]
    };
    show(50, h(2));
    show(51, b[0] + b[1] + b[2]);
}

fn operators() {
    let a = S { v: 1 };
    let b = S { v: 2 };
    let ra = &a;
    let rb = &b;
    let f = || (*ra + *rb).v;
    show(60, f());
    let f = move || (*ra + *rb).v;
    show(61, f());
    let f = || ra == rb;
    show(62, f() as i32);
    let f = || *ra == *rb;
    show(63, f() as i32);
    let x = 5;
    let rx = &x;
    let f = || rx + 1;
    show(64, f());
    let f = || rx == &5;
    show(65, f() as i32);
    let mut y = 3;
    let mut z = 4;
    let my = &mut y;
    let mz = &mut z;
    let f = || my == mz;
    show(66, f() as i32);
    let mut g = || my < mz;
    show(67, g() as i32);
    *my += 1;
    show(68, *my + *mz);
}

fn patterns() {
    let s = S { v: 3 };
    let r = &s;
    let f = || match r {
        S { v } => *v,
    };
    show(70, f());
    let f = move || match r {
        S { v } => *v + 1,
    };
    show(71, f());
    let f = || {
        let S { v } = r;
        *v * 2
    };
    show(72, f());
    let f = || {
        let &S { v } = r;
        v
    };
    show(73, f());
    let f = || {
        let _ = r;
        5
    };
    show(74, f());
    let f = || match r {
        _ => 6,
    };
    show(75, f());

    let e = E::B { x: 9 };
    let re = &e;
    let f = || match re {
        E::A(a) => *a,
        E::B { x } => *x,
    };
    show(76, f());
    let f = || if let E::B { x } = re { *x } else { 0 };
    show(77, f());
    let o = Some(S { v: 2 });
    let ro = &o;
    let f = || match ro {
        Some(S { v }) if *v < 3 => *v,
        Some(_) => -1,
        None => 0,
    };
    show(78, f());
    let f = || {
        let Some(p) = ro else { return 0 };
        p.area()
    };
    show(79, f());
    let t = (1, (2, 3));
    let rt = &t;
    let f = || {
        let (a, (b, c)) = rt;
        a + b + c
    };
    show(80, f());
    let f = || match rt {
        (1, (b, _)) => *b,
        _ => 0,
    };
    show(81, f());
    let f = || match &rt.1 {
        (2, c) => *c,
        _ => 0,
    };
    show(82, f());

    let mut pair = (1, 2);
    let mp = &mut pair;
    let mut f = || {
        let (a, _) = mp;
        *a += 10;
    };
    f();
    let mut g = || match mp {
        (a, b) if *a > *b => {
            *b += 1;
            *b
        }
        _ => 0,
    };
    show(83, g());
    show(84, pair.0 + pair.1);
}

fn nested_and_calls() {
    let s = S { v: 2 };
    let r = &s;
    let f = || {
        let g = |k: i32| r.get() * k;
        let h = move || r.v;
        g(3) + h()
    };
    show(90, f());
    let f = move || {
        let g = || r.get();
        g() + 1
    };
    show(91, f());
    let rr = &r;
    let f = || {
        let g = || rr.area();
        g()
    };
    show(92, f());

    let double = |x: i32| x * 2;
    let rd = &double;
    let f = || rd(5);
    show(93, f());
    let f = move || rd(6);
    show(94, f());
    show(95, call_plus_one(&double, 7));
    let mut n = 0;
    let mut counter = || {
        n += 1;
        n
    };
    let mc = &mut counter;
    let mut f = || mc() + mc();
    show(96, f());
    show(97, n);

    let mut c = Counter { n: 1, step: 2 };
    show(98, c.sum_with(10));
    c.bump_twice();
    show(99, c.get());
    show(100, c.bump_and_get());
    show(101, c.mapped());
}

fn adapters() {
    let table = Table {
        values: [10, 20, 30, 40],
    };
    let tr = &table;
    let sum: i32 = (0..4).map(|i| table.lookup(i)).sum();
    show(110, sum);
    let sum: i32 = (0..4).map(|i| tr.lookup(i)).sum();
    show(111, sum);
    let sum: i32 = (0..4).map(move |i| tr.lookup(i)).sum();
    show(112, sum);
    let xs = [10, 15, 20, 25];
    let xs: &[i32] = &xs;
    let count = xs.iter().filter(|x| tr.has(**x)).count();
    show(113, count as i32);
    let count = (0..6).filter(|i| *i < xs.len()).count();
    show(114, count as i32);

    let sieve = Sieve::new();
    let sr = &sieve;
    let n = 16;
    let p = (2..n).find(|&p| sr.is_prime(p) && sr.is_prime(n - p));
    show(115, p.unwrap_or(0) as i32);
    let p = (2..n).find(|&p| sieve.is_prime(p) && sieve.is_prime(n - p));
    show(116, p.unwrap_or(0) as i32);
    let p = (2..n).find(move |&p| sr.is_prime(p) && sr.is_prime(n - p));
    show(117, p.unwrap_or(0) as i32);

    let v = [1, 2, 3];
    let vr: &[i32] = &v;
    let any = vr.iter().any(|&x| x == vr.len() as i32);
    show(118, any as i32);
    let folded = vr.iter().map(|x| x * vr[2]).fold(0, |a, b| a + b);
    show(119, folded);
    let f = || vr.iter().filter(|x| vr.len() as i32 > **x).count() as i32;
    show(120, f());
    let f = move || vr.iter().map(|x| x * vr[0]).sum::<i32>();
    show(121, f());
    let f = || {
        let mut t = 0;
        for x in vr.iter() {
            t += *x;
        }
        for x in vr {
            t += x;
        }
        t
    };
    show(122, f());
    let mut it = vr.iter();
    let mut g = || {
        let mut t = 0;
        while let Some(x) = it.next() {
            t += x;
        }
        t
    };
    show(123, g());
}

fn main() {
    receivers();
    reborrows();
    fields_tuples_arrays();
    slices();
    operators();
    patterns();
    nested_and_calls();
    adapters();
}
