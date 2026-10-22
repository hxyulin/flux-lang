// Borrow checking with closures: a closure holds the borrows of what it
// captures for as long as it may be called, through the values it is
// moved into; what its body may do with them follows from its kind.

struct S {
    a: i32,
    b: i32,
}

struct Noisy(i32);

fn id<F>(f: F) -> F {
    f
}

fn fn_mut<F: FnMut()>(mut f: F) {
    f();
}

fn fn_only<F: Fn()>(f: F) {
    f();
}

fn assign_while_borrowed() {
    let mut x = 1;
    let c = || x + 1;
    x = 5;
    c();
}

fn mutable_twice() {
    let mut v = 0;
    let mut a = || v += 1;
    let mut b = || v += 2;
    a();
    b();
}

fn read_while_mut() {
    let mut v = 0;
    let mut inc = || v += 1;
    let r = v;
    inc();
    let _ = r;
}

fn through_call() {
    let mut x = 1;
    let c = id(|| x);
    x = 2;
    c();
}

fn through_iterator() {
    let mut k = 0;
    let it = (0..3).map(|x| x + k);
    k = 5;
    for _ in it {}
}

fn called_twice() {
    let s = S { a: 1, b: 2 };
    let c = move || {
        let t = s;
        t.a
    };
    c();
    c();
}

fn not_mut() {
    let mut x = 0;
    let c = || x += 1;
    c();
}

fn expected_fn() {
    let mut x = 0;
    fn_only(|| x += 1);
}

fn move_not_mut() {
    let x = 0;
    let mut c = move || x += 1;
    c();
}

fn ref_not_mut() {
    let x = 0;
    let mut c = || x += 1;
    c();
}

fn moved_in_fn_mut() {
    let n = Noisy(1);
    fn_mut(|| drop(n));
}

fn field_conflict() {
    let mut s = S { a: 1, b: 2 };
    let mut c = || s.a += 1;
    let r = &s.a;
    c();
    let _ = r;
}

fn out_of_scope() {
    let c;
    {
        let x = 5;
        c = || x;
    }
    c();
}

fn main() {}
