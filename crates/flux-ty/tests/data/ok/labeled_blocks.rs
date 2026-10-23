fn choose(early: bool, n: i32) -> i32 {
    'alias: {
        if early { break 'alias n + 10; }
        n + 20
    }
}
fn nested(n: i32) -> i32 {
    'outer: {
        let mut i = 0;
        'scan: loop {
            i += 1;
            'inner: {
                if i < n { continue 'scan; }
                if i == n { break 'outer i * 3; }
                break 'inner;
            }
            break;
        }
        99
    }
}
fn shadow() -> i32 {
    'same: {
        let x = 'same: { break 'same 7; };
        break 'same x + 1;
    }
}
fn slice<'a>(early: bool, a: &'a [i32; 2], b: &'a [i32; 3]) -> &'a [i32] {
    'value: { if early { break 'value a; } b }
}
fn main() {}
