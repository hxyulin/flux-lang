// Malformed code rustc reports with one error, at what is wrong.

fn patterns(x: i32) {
    match x {
        1 | => {}
        -y => {}
        _ => {}
    }
}

fn let_else(o: Option<i32>) {
    let Some(a) = o else return;
    let Some(b) = o else if true { return; } else { return; };
}

fn if_else(c: bool) -> i32 {
    if c { 1 } else 2
}

fn missing_comma(x: i32) -> i32 {
    match x {
        1 => 2
        2 => 3,
        _ => 4,
    }
}
