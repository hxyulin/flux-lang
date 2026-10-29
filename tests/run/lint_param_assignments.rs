// A parameter counts as assigned on entry, so the overflow lint does not
// carry a value assigned on one path to the code after the branch.

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
    println!("{}", assigned_on_one_path(1, false));
    println!("{}", assigned_in_loop(-3, 0));
}
