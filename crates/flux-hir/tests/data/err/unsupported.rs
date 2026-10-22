struct S;

extern "C" {
    fn abs(x: i32) -> i32;
}

fn generic<T>(x: T) -> &'static str {
    "text"
}

#[cfg_attr(all(), inline)]
fn attributes() {}

fn expressions(v: i32) {
    let r = &v;
    'outer: loop {
        break 'outer;
    }
    for i in 0..3 {}
    match v {
        _ => {}
    }
    let f = 1.5;
    let (a, b) = (1, 2);
    v.method();
}

fn declaration_only();
