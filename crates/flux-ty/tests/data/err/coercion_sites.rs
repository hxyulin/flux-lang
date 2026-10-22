// Mismatches at coercion sites inside tuples, arrays and branches are
// reported where rustc reports them: at the element or branch.

fn takes(_: (i32, &[i32])) {}

fn in_array() {
    let _t: [(i32, &[i32]); 1] = [(1, &true)];
}

fn through_ref() {
    let _t: (i32, &[u8]) = (1, &[1i32, 2]);
}

fn arity() {
    let a = [1, 2];
    takes((1, &a, 3));
}

fn lengths(c: bool) {
    let a = [1, 2, 3];
    let b = [1, 2];
    let _x = if c { &a } else { &b };
}

fn closure_return() {
    let _f: fn(i32) -> i32 = |x: i32| x == 1;
}

fn shared_to_mut() {
    let a = [1, 2];
    let _r: &mut [i32] = &a;
}

fn later_arm(c: i32) {
    let _x: (i32, &[i32]) = match c {
        0 => (1, &[1]),
        _ => (true, &[2]),
    };
}

fn first_branch(c: bool) {
    let _x: i32 = if c { false } else { 1 };
    let _y: i32 = match c {
        true => 1,
        false => false,
    };
}

fn shared_to_mut_ptr() {
    let x = 1;
    let _t: (i32, *mut i32) = (1, &x);
}

fn bytes_in_tuple() {
    let _t: [(u8, &[i32]); 1] = [(1, b"ab")];
}

fn cast_mismatch() {
    let a = [1u8, 2];
    let _s = &a as &[i32];
}

fn main() {}
