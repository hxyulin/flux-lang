// Slice patterns that do not cover every length or value (E0004, E0005),
// reported with rustc's witnesses: lengths up to the longest pattern,
// then those at least as long, `[_, ..]`; arrays as fixed lengths.

fn a(s: &[u8]) {
    match s {
        [x, y] => {}
    }
}

fn b(s: &[u8]) {
    match s {
        [] => {}
    }
}

fn c(s: &[bool]) {
    match s {
        [] => {}
        [true, ..] => {}
        [.., false] => {}
    }
}

fn d(a: [bool; 2]) {
    match a {
        [true, _] => {}
        [_, true] => {}
    }
}

fn e(s: &[u8]) {
    match s {
        [_, ..] => {}
    }
}

fn f(s: &[u8]) {
    let [x, ..] = s;
}

fn main() {}
