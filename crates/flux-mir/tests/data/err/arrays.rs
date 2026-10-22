// Borrows and moves of arrays and slices, as rustc
// reports them.

struct S;

fn move_out(v: [S; 2]) -> S {
    v[0]
}

fn move_out_of_slice(s: &[S]) -> S {
    s[0]
}

fn two_mutable(mut a: [i32; 3]) {
    let r = &mut a[0];
    let q = &mut a[1];
    *r = 1;
    *q = 2;
}

fn assign_borrowed(mut a: [i32; 3]) {
    let r = &a[0];
    a[1] = 5;
    let _ = *r;
}

fn len_while_borrowed(mut a: [i32; 3]) {
    let m = &mut a;
    let n = a.len();
    m[0] = n as i32;
}

fn use_while_borrowed(s: &mut [i32]) {
    let r = &mut s[0];
    let x = s[1];
    *r = x;
}

fn slice_len_while_borrowed(s: &mut [i32]) {
    let r = &mut s[0];
    let n = s.len();
    *r = n as i32;
}

fn not_mutable(a: [i32; 3]) {
    a[0] = 1;
    let r = &mut a[1];
    *r = 2;
}

fn behind_shared(s: &[i32]) {
    s[0] = 1;
}

fn uninit() {
    let a: [i32; 3];
    a[0] = 1;
}

fn main() {}
