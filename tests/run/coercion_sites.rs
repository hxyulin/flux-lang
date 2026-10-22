//! Every coercion flux supports at every coercion site: unsizing
//! (`&[T; N]` to `&[T]`, from `&` and `&mut`, byte strings included),
//! `&mut T` to `&T`, reborrows, deref coercion (`&&T` to `&T`),
//! references to raw pointers, function items and closures that capture
//! nothing to function pointers, and `!` to any type; in `let`s with
//! types, function and method arguments, return values, struct, enum
//! variant, tuple and array fields (nested, as a tuple in an array in an
//! `Option`), `if` and `match` branches (with a type wanted, and without,
//! as rustc's least upper bound), array repeats, `break` values,
//! closures' returns, assignments, `as` casts, statics and constants. Each prints what it got, so a value built with the type it
//! had before its coercion (a slice with a garbage length) shows.

unsafe extern "C" {
    fn printf(format: *const i8, ...) -> i32;
}

fn double(x: i32) -> i32 {
    x * 2
}

fn negate(x: i32) -> i32 {
    -x
}

static A: [i32; 4] = [1, 2, 3, 4];
static S: &[i32] = &A;
const C: &[i32] = &[1, 2, 3];
static T: (i32, &[i32]) = (1, &A);
const U: (i32, &[i32]) = (1, &[5, 6]);
const G: [&[i32]; 2] = [&[1], &[1, 2]];

fn lets() {
    let a = [1, 2, 3, 4, 5];
    let mut m = [7, 8, 9];
    let mut n = 10;
    let s: &[i32] = &a;
    let b: &[i32] = { &a };
    let ms: &mut [i32] = &mut m;
    ms[0] = 70;
    let ss: &[i32] = &mut m;
    let (ss0, ssl) = (ss[0], ss.len());
    let bb: &[i32] = { { &m } };
    let r: &i32 = &mut n;
    let f: fn(i32) -> i32 = double;
    let g: fn(i32) -> i32 = |x| x + 100;
    let t: (i32, &[i32]) = { (1, &a) };
    let tt: (i32, &[i32]) = {
        let q = 2;
        (q, &a)
    };
    let nested: ((i32, &[i32]), i32) = ((1, &a), 2);
    unsafe {
        printf(
            c"let: %zu %zu %zu %zu %d %d %d %d\n".as_ptr(),
            s.len(),
            b.len(),
            bb.len(),
            ssl,
            ss0,
            *r,
            f(4),
            g(4),
        );
        printf(
            c"let: %zu %zu %d %zu\n".as_ptr(),
            t.1.len(),
            tt.1.len(),
            tt.0,
            (nested.0).1.len(),
        );
    }
}

fn len(s: &[i32]) -> usize {
    s.len()
}

fn first_mut(s: &mut [i32]) -> i32 {
    s[0] += 1;
    s[0]
}

fn read(r: &i32) -> i32 {
    *r
}

fn bump(r: &mut i32) {
    *r += 1;
}

fn apply(f: fn(i32) -> i32, x: i32) -> i32 {
    f(x)
}

fn tup(t: (i32, &[i32])) -> usize {
    t.1.len() + t.0 as usize
}

fn nested(t: ((i32, &[i32]), &[i32])) -> usize {
    (t.0).1.len() * 10 + t.1.len()
}

fn opt(o: Option<(i32, &[i32])>) -> usize {
    match o {
        Some((_, s)) => s.len(),
        None => 0,
    }
}

fn arr(t: [&[i32]; 2]) -> usize {
    t[0].len() * 10 + t[1].len()
}

fn generic<X>(x: X) -> X {
    x
}

fn args() {
    let a = [1, 2, 3, 4, 5];
    let mut m = [7, 8, 9];
    let mut n = 10;
    let r = &mut n;
    // Reborrowed, so `r` is still usable.
    bump(r);
    bump(r);
    let x = read(r);
    let mr = &mut m;
    let y = first_mut(mr);
    let z = first_mut(mr);
    let rr = &&n;
    let g: &[i32] = generic(&a);
    let h: (i32, &[i32]) = generic((3, &a));
    unsafe {
        printf(
            c"args: %zu %zu %d %d %d %d %d\n".as_ptr(),
            len(&a),
            len(&mut m),
            x,
            y,
            z,
            n,
            read(rr),
        );
        printf(
            c"args: %d %d %zu %zu %zu %zu %zu %zu\n".as_ptr(),
            apply(double, 5),
            apply(|v| v - 1, 5),
            tup((1, &a)),
            nested(((1, &a), &m)),
            opt(Some((1, &a))),
            arr([&a, &m]),
            g.len(),
            h.1.len(),
        );
    }
}

struct K {
    k: usize,
}

impl K {
    fn len(&self, s: &[i32]) -> usize {
        s.len() + self.k
    }

    fn tup(&self, t: (i32, &[i32])) -> usize {
        t.1.len() + self.k
    }

    fn read(&self, r: &i32) -> i32 {
        *r
    }

    fn apply(&self, f: fn(i32) -> i32) -> i32 {
        f(self.k as i32)
    }

    fn arr(&self, t: [(u8, &[i32]); 2]) -> usize {
        t[0].1.len() * 10 + t[1].1.len()
    }
}

fn methods() {
    let a = [1, 2, 3, 4, 5];
    let m = [7, 8, 9];
    let mut n = 4;
    let k = K { k: 100 };
    unsafe {
        printf(
            c"methods: %zu %zu %d %d %d %zu\n".as_ptr(),
            k.len(&a),
            k.tup((1, &m)),
            k.read(&mut n),
            k.apply(double),
            k.apply(|x| x + 1),
            k.arr([(1, &a), (2, &m)]),
        );
    }
}

fn tail() -> &'static [i32] {
    &A
}

fn early(c: bool) -> &'static [i32] {
    if c {
        return &A;
    }
    &[1, 2]
}

fn tup_tail() -> (i32, &'static [i32]) {
    (1, &A)
}

fn tup_return() -> (i32, &'static [i32]) {
    return (2, &A);
}

fn opt_tail() -> Option<(i32, &'static [i32])> {
    Some((3, &A))
}

fn fn_ptr() -> fn(i32) -> i32 {
    double
}

fn closure_ptr() -> fn(i32) -> i32 {
    |x| x + 7
}

fn down(r: &mut i32) -> &i32 {
    r
}

fn arr_tail() -> [&'static [i32]; 2] {
    [&A, &[9]]
}

fn block_tail() -> (i32, &'static [i32]) {
    let x = 5;
    { (x, &A) }
}

fn diverge(o: Option<i32>) -> (i32, &'static [i32]) {
    let v: i32 = match o {
        Some(v) => v,
        None => return (0, &[0, 0]),
    };
    (v, &A)
}

fn returns() {
    let mut n = 3;
    unsafe {
        printf(
            c"returns: %zu %zu %zu %zu %zu %zu\n".as_ptr(),
            tail().len(),
            early(true).len(),
            early(false).len(),
            tup_tail().1.len(),
            tup_return().1.len(),
            opt_tail().unwrap().1.len(),
        );
        printf(
            c"returns: %d %d %d %zu %zu %zu %zu %zu\n".as_ptr(),
            fn_ptr()(4),
            closure_ptr()(4),
            *down(&mut n),
            arr_tail()[0].len(),
            arr_tail()[1].len(),
            block_tail().1.len(),
            diverge(Some(1)).1.len(),
            diverge(None).1.len(),
        );
    }
}

struct F<'a> {
    s: &'a [i32],
    t: (u8, &'a [i32]),
    f: fn(i32) -> i32,
    r: &'a i32,
}

struct Pair<'a>(&'a [i32], (i32, &'a [i32]));

enum E<'a> {
    A(&'a [i32]),
    B { t: (i32, &'a [i32]) },
    C(fn(i32) -> i32),
}

struct Wrap<X> {
    x: X,
}

fn e_len(e: &E) -> usize {
    match e {
        E::A(s) => s.len(),
        E::B { t } => t.1.len() * 100,
        E::C(f) => (*f)(1) as usize,
    }
}

fn fields() {
    let a = [1, 2, 3, 4, 5];
    let m = [7, 8, 9];
    let mut n = 6;
    let s = F {
        s: &a,
        t: (1, &m),
        f: double,
        r: &mut n,
    };
    let p = Pair(&m, (4, &a));
    let e1 = E::A(&a);
    let e2 = E::B { t: (1, &m) };
    let e3 = E::C(|x| x + 41);
    let e4 = E::C(double);
    let g: Wrap<&[i32]> = Wrap { x: &a };
    let h: Wrap<(i32, &[i32])> = Wrap { x: (1, &m) };
    let table: [(i32, &[i32]); 2] = [(1, &a), (2, &m)];
    let grid: [[&[i32]; 2]; 1] = [[&a, &m]];
    let fs: [fn(i32) -> i32; 2] = [double, |x| x - 1];
    let tt: ((i32, &[i32]), (&[i32], i32)) = ((1, &a), (&m, 2));
    let o: Option<[(i32, &[i32]); 2]> = Some([(1, &a), (2, &m)]);
    let oo: [Option<(i32, &[i32])>; 2] = [Some((1, &a)), None];
    let ft: (fn(i32) -> i32, &i32, &[i32]) = (negate, &&a[0], &mut [0; 4]);
    unsafe {
        printf(
            c"fields: %zu %zu %d %d %zu %zu\n".as_ptr(),
            s.s.len(),
            s.t.1.len(),
            (s.f)(3),
            *s.r,
            p.0.len(),
            (p.1).1.len(),
        );
        printf(
            c"fields: %zu %zu %zu %zu %zu %zu\n".as_ptr(),
            e_len(&e1),
            e_len(&e2),
            e_len(&e3),
            e_len(&e4),
            g.x.len(),
            h.x.1.len(),
        );
        printf(
            c"fields: %zu %zu %zu %zu %d %d %zu %zu\n".as_ptr(),
            table[0].1.len(),
            table[1].1.len(),
            grid[0][0].len(),
            grid[0][1].len(),
            fs[0](5),
            fs[1](5),
            (tt.0).1.len(),
            (tt.1).0.len(),
        );
        let o = o.unwrap();
        printf(
            c"fields: %zu %zu %zu %d %d %d %zu\n".as_ptr(),
            o[0].1.len(),
            o[1].1.len(),
            oo[0].unwrap().1.len(),
            oo[1].is_none() as i32,
            (ft.0)(2),
            *ft.1,
            ft.2.len(),
        );
    }
}

fn pick(c: bool) -> i32 {
    let x: i32 = if c { 5 } else { return 9 };
    x
}

fn branches(c: bool, k: i32) {
    let a = [1, 2, 3, 4, 5];
    let b = [7, 8, 9];
    let sl: &[i32] = &a;
    let mut n = 6;
    let s: &[i32] = if c { &a } else { &b };
    let t: (i32, &[i32]) = if c { (1, &a) } else { (2, &b) };
    let u: &[i32] = match k {
        1 => &a,
        2 => &b,
        _ => &[],
    };
    let v: (i32, &[i32]) = match k {
        1 => (1, &a),
        _ => (2, &b),
    };
    let f: fn(i32) -> i32 = if c { double } else { negate };
    // Without a type: the least upper bound.
    let g = if c { double } else { negate };
    let h: fn(i32) -> i32 = match k {
        1 => double,
        2 => |x| x + 1,
        _ => negate,
    };
    let hh = match k {
        1 => |x: i32| x * 3,
        _ => negate,
    };
    let r: &i32 = if c { &mut n } else { &k };
    let w = if c { sl } else { &b };
    let ww = match k {
        1 => &b,
        _ => sl,
    };
    let rr = &&k;
    let d: &i32 = if c { rr } else { &a[0] };
    unsafe {
        printf(
            c"branches: %zu %zu %zu %zu %d %d %d %d\n".as_ptr(),
            s.len(),
            t.1.len(),
            u.len(),
            v.1.len(),
            f(3),
            g(3),
            h(3),
            hh(3),
        );
        printf(
            c"branches: %d %zu %zu %d %d\n".as_ptr(),
            *r,
            w.len(),
            ww.len(),
            *d,
            pick(c),
        );
    }
}

fn misc() {
    let a = [1, 2, 3, 4, 5];
    let b = [7, 8, 9];
    let rep: [&[i32]; 3] = [&a; 3];
    let rept: [(i32, &[i32]); 2] = [(1, &b); 2];
    let mut i = 0;
    let br: &[i32] = loop {
        i += 1;
        if i > 2 {
            break &a;
        }
    };
    let brt: (i32, &[i32]) = loop {
        break (1, &b);
    };
    let mut s: &[i32] = &b;
    let l1 = s.len();
    s = &a;
    let mut t: (i32, &[i32]) = (0, &b);
    let l2 = t.1.len();
    t = (1, &a);
    let mut f: fn(i32) -> i32 = double;
    let f1 = f(1);
    f = |x| x + 10;
    let c1 = || -> &[i32] { &a };
    let c2 = || -> (i32, &[i32]) { (1, &b) };
    let c3 = |c: bool| -> &[i32] {
        if c {
            return &a;
        }
        &b
    };
    unsafe {
        printf(
            c"misc: %zu %zu %zu %zu %zu %zu %zu %zu %d %d\n".as_ptr(),
            rep[2].len(),
            rept[1].1.len(),
            br.len(),
            brt.1.len(),
            l1,
            s.len(),
            l2,
            t.1.len(),
            f1,
            f(1),
        );
        printf(
            c"misc: %zu %zu %zu %zu\n".as_ptr(),
            c1().len(),
            c2().1.len(),
            c3(true).len(),
            c3(false).len(),
        );
        printf(
            c"statics: %zu %zu %zu %zu %zu %zu\n".as_ptr(),
            S.len(),
            C.len(),
            T.1.len(),
            U.1.len(),
            G[0].len(),
            G[1].len(),
        );
    }
}

fn take(p: *const i32) -> i32 {
    unsafe { *p }
}

fn take_mut(p: *mut i32) {
    unsafe { *p += 1 }
}

fn byte_len(s: &[u8]) -> usize {
    s.len()
}

fn byte_tup(t: (u8, &[u8])) -> usize {
    t.1.len()
}

struct Raw {
    p: *const i32,
    q: (*mut i32, u8),
}

const BYTES: &[u8] = b"const";
static BYTE_TUPLE: (i32, &[u8]) = (1, b"static");

fn raw_and_bytes(c: bool, k: i32) {
    let x = 5;
    let mut y = 7;
    let p: *const i32 = &x;
    let q: *const i32 = &mut y;
    let t: (i32, *const i32) = (1, &x);
    let arr: [*const i32; 2] = [&x, &y];
    let o: Option<(*const i32, u8)> = Some((&x, 1));
    let r: *const i32 = if c { &x } else { &y };
    let m: *const i32 = match k {
        1 => &y,
        _ => &x,
    };
    let a = take(&x);
    take_mut(&mut y);
    let raw = Raw {
        p: &x,
        q: (&mut y, 2),
    };
    let s: &[u8] = b"hello";
    let bt: (u8, &[u8]) = (1, b"abc");
    let ba: [(u8, &[u8]); 2] = [(1, b"ab"), (2, b"abcd")];
    let bo: Option<(u8, &[u8])> = Some((1, b"xyz12"));
    let bn: ((u8, &[u8]), u8) = ((1, b"nested"), 2);
    let bi: &[u8] = if c { b"yes" } else { b"no!!" };
    let bm = match k {
        1 => b"five" as &[u8],
        _ => b"other",
    };
    let br: &[u8] = loop {
        break b"brk";
    };
    unsafe {
        printf(
            c"raw: %d %d %d %d %d %d %d %d %d %d %d\n".as_ptr(),
            *p,
            *q,
            *t.1,
            *arr[1],
            *o.unwrap().0,
            *r,
            *m,
            a,
            y,
            *raw.p,
            *raw.q.0,
        );
        printf(
            c"bytes: %zu %zu %zu %zu %zu %zu %zu %zu %zu\n".as_ptr(),
            s.len(),
            bt.1.len(),
            ba[0].1.len(),
            ba[1].1.len(),
            bo.unwrap().1.len(),
            (bn.0).1.len(),
            bi.len(),
            bm.len(),
            br.len(),
        );
        printf(
            c"bytes: %zu %zu %zu %zu\n".as_ptr(),
            byte_len(b"four"),
            byte_tup((1, b"three")),
            BYTES.len(),
            BYTE_TUPLE.1.len(),
        );
    }
}

fn ranges(c: bool) {
    let a = [1, 2, 3, 4, 5];
    let b = [7, 8, 9];
    // A slice and an array reference meet at the slice, either way round.
    let w = if c { &a[1..] } else { &b };
    let x = if c { &b } else { &a[..2] };
    let y = match a[1] {
        2 => &b,
        _ => &a[..],
    };
    let t: [(i32, &[i32]); 2] = [(1, &a[..1]), (2, &b)];
    unsafe {
        printf(
            c"ranges: %zu %zu %zu %zu %zu\n".as_ptr(),
            w.len(),
            x.len(),
            y.len(),
            t[0].1.len(),
            t[1].1.len(),
        );
    }
}

fn main() {
    lets();
    args();
    methods();
    returns();
    fields();
    branches(true, 1);
    branches(false, 2);
    branches(false, 3);
    misc();
    raw_and_bytes(true, 1);
    raw_and_bytes(false, 2);
    ranges(true);
    ranges(false);
}
