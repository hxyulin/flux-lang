// Closures that match an element of an array borrow the array, even
// with only wildcards, mutably for `ref mut`: conflicts with it while
// the closure lives.

fn wildcard_let() {
    let mut p = [1, 2];
    let c = || {
        let _ = p[0];
    };
    p[1] = 5;
    c();
}

fn wildcard_match() {
    let mut p = [(1, 2), (3, 4)];
    let c = || match p[1].0 {
        _ => {}
    };
    let r = &mut p;
    c();
    let _ = r;
}

fn ref_mut_binding() {
    let mut p = [1, 2];
    let mut c = || match p[0] {
        ref mut x => *x += 1,
    };
    let v = p[1];
    c();
    let _ = v;
}

fn let_ref_mut() {
    let mut p = [(1, 2), (3, 4)];
    let mut c = || {
        let (ref mut a, _) = p[1];
        *a = 0;
    };
    let r = &p;
    c();
    let _ = r;
}

fn main() {
    wildcard_let();
    wildcard_match();
    ref_mut_binding();
    let_ref_mut();
}
