fn holder_of_capture() {
    let a = 1;
    let r0 = &a;
    let mut holder: &&i32 = &r0;
    {
        let s = &a;
        let mut c = || {
            holder = &s;
        };
        c();
    }
    let _ = **holder;
}

fn annotated_identity() {
    let c = |x: &i32| x;
    let a = 1;
    let _ = c(&a);
}

fn inferred_identity() {
    let c = |x| x;
    let a = 1;
    let r = c(&a);
    let _ = *r;
}

fn call_it<F: Fn() -> i32>(f: F) -> i32 {
    f()
}

fn returns_upvar_passed() -> i32 {
    let v = 5;
    let r = &v;
    call_it(|| *r)
}

fn fn_mut_escape() {
    let mut x = 1;
    let mut c = || &mut x;
    let _ = c();
}

fn fn_returning_capture() {
    let x = 1;
    let r;
    {
        let y = 2;
        let c = || if x > 0 { &x } else { &y };
        r = c();
    }
    let _ = *r;
}

fn main() {}
