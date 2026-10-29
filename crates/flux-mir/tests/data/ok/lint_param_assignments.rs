// A parameter counts as assigned on entry, so one more assignment makes
// its known value local to the block that assigns it.
fn assigned_on_one_path(mut x: u8, c: bool) -> u8 {
    if c {
        x = 255;
    }
    x + 1
}

fn assigned_in_loop(mut x: i8, n: u32) -> i8 {
    for _ in 0..n {
        x = 127;
    }
    x + 1
}

fn main() {
    let _ = assigned_on_one_path(1, false);
    let _ = assigned_in_loop(1, 0);
}
