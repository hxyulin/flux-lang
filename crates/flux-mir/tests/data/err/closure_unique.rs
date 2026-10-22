// Conflicts with closures' unique captures, of a place behind a `&mut`
// in a binding that is not `mut` (E0500, E0501, E0524), and with their
// other mutable captures.

fn shared() {
    let mut x = 0;
    let r = &mut x;
    let mut c = || *r += 1;
    let y = &r;
    c();
    let _ = y;
}

fn mutable() {
    let mut x = 0;
    let mut r = &mut x;
    let mut c = || *r += 1;
    let y = &mut r;
    c();
    let _ = y;
}

fn read() {
    let mut x = 0;
    let r = &mut x;
    let mut c = || *r += 1;
    let v = *r;
    c();
    let _ = v;
}

fn second_closure() {
    let mut x = 0;
    let r = &mut x;
    let mut c = || *r += 1;
    let mut d = || *r += 2;
    c();
    d();
}

fn shared_mut_binding() {
    let mut x = 0;
    let mut r = &mut x;
    let mut c = || *r += 1;
    let y = &r;
    c();
    let _ = y;
    r = &mut x;
    let _ = r;
}

fn second_closure_mut_binding() {
    let mut x = 0;
    let mut r = &mut x;
    let mut c = || *r += 1;
    let mut d = || *r += 2;
    c();
    d();
    r = &mut x;
    let _ = r;
}

fn plain_local() {
    let mut x = 0;
    let mut c = || x += 1;
    let y = &x;
    c();
    let _ = y;
}

fn plain_two() {
    let mut x = 0;
    let mut c = || x += 1;
    let mut d = || x += 2;
    c();
    d();
}

fn shared_then_closure() {
    let mut x = 0;
    let r = &mut x;
    let y = &r;
    let mut c = || *r += 1;
    c();
    let _ = y;
}

fn main() {}
