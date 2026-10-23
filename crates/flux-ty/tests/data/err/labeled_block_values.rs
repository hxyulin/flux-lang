fn mismatch(early: bool) -> i32 {
    'value: { if early { break 'value true; } 1 }
}
fn missing(early: bool) -> i32 {
    'value: { if early { break 'value 1; } }
}
fn unit(early: bool) -> i32 {
    'value: { if early { break 'value; } 1 }
}
fn main() {}
