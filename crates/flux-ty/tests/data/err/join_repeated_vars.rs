// `(a, a)` and `(i32, u8)` do not unify: once `a` is `i32` it is not
// `u8`. Each element looked fine on its own (E0308).

fn branches(c: bool) {
    let a = Default::default();
    let t = if c { (a, a) } else { (1i32, 2u8) };
}

fn arms(n: i32) {
    let a = Default::default();
    let t = match n {
        0 => (a, a),
        _ => (1i32, 2u8),
    };
}

fn main() {}
