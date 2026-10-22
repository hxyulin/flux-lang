struct Holder<'a> {
    r: &'a i32,
}

fn ret_local() -> &'static i32 {
    let x = 1;
    &x
}

fn ret_local_value() -> Option<&'static i32> {
    let x = 1;
    Some(&x)
}

fn ret_param(x: i32) -> &'static i32 {
    &x
}

fn ret_temp() -> &'static i32 {
    &id(1)
}

fn id(x: i32) -> i32 {
    x
}

fn wrong_lifetime<'a, 'b>(x: &'a i32, y: &'b i32) -> &'a i32 {
    y
}

fn dangling() {
    let r;
    {
        let x = 1;
        r = &x;
    }
    let _ = *r;
}

fn dangling_struct() {
    let h;
    {
        let x = 1;
        h = Holder { r: &x };
    }
    let _ = *h.r;
}

fn assign_while_returned_borrowed() {
    let mut x = 1;
    let r = longest(&x, &x);
    x = 2;
    let _ = *r;
}

fn longest<'a>(a: &'a i32, b: &'a i32) -> &'a i32 {
    if *a > *b { a } else { b }
}

fn mut_ref_rebind() {
    let mut a = 1;
    let b = 2;
    let mut r = &a;
    let _ = *r;
    r = &b;
    a = 5;
    let _ = *r + a;
}

fn mut_ref_bad() {
    let mut a = 1;
    let r = &mut a;
    let s = &a;
    *r = 2;
    let _ = *s;
}

fn store_into_param<'a>(slot: &mut &'a i32) {
    let x = 1;
    *slot = &x;
}

fn temp_dropped() {
    let r: &i32;
    r = &id(3);
    let _ = *r;
}

fn main() {}
