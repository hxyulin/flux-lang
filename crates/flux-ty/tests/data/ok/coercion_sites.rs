// Coercions inside tuples, arrays and branches take the type wanted:
// each tuple element, array element, `if` and `match` branch and `break`
// value is a coercion site when its context wants a type, and branches
// without one meet at their least upper bound.

fn double(x: i32) -> i32 {
    x * 2
}

fn negate(x: i32) -> i32 {
    -x
}

struct K;

impl K {
    fn tup(&self, t: (i32, &[i32])) -> usize {
        t.1.len()
    }
}

fn tuples(a: &[i32; 3], b: &mut [i32; 2]) {
    let _t: [(i32, &[i32]); 1] = [(10, a)];
    let _o: Option<(i32, &[i32])> = Some((1, a));
    let _n: ((i32, &[i32]), i32) = ((1, a), 2);
    let _m: (fn(i32) -> i32, &[i32]) = (double, b);
    let _r: &(i32, &[i32]) = &(1, a);
    let _s: &[&[i32]] = &[a, &[1, 2, 3, 4]];
    let _k = K.tup((1, a));
    let _p: (i32, *const i32) = (1, &a[0]);
    let _q: Option<(*mut i32, u8)> = Some((&mut b[0], 1));
    let _b: [(u8, &[u8]); 2] = [(1, b"ab"), (2, b"abc")];
    let _c = a as &[i32];
}

fn branches(c: bool, a: &[i32; 3], b: &[i32; 2], sl: &[i32]) {
    let _s: &[i32] = if c { a } else { b };
    let _t: (i32, &[i32]) = if c { (1, a) } else { (2, b) };
    let _u: &[i32] = match c {
        true => a,
        false => b,
    };
    let _f = if c { double } else { negate };
    let _g = match c {
        true => |x: i32| x + 1,
        false => double,
    };
    let _h = match c {
        true => b,
        false => sl,
    };
    let _l: (i32, &[i32]) = loop {
        break (1, a);
    };
}

fn main() {
    let mut b = [4, 5];
    tuples(&[1, 2, 3], &mut b);
    branches(true, &[1, 2, 3], &b, &b);
}
