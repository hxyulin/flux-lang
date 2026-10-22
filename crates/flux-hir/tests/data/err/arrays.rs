// Array lengths that are not constants, and slice patterns.

const N: usize = 2;

fn lengths(n: usize) {
    let a = [0; n];
    let b: [u8; N + 1] = [0; 3];
    let c = [0; { 3 }];
    let d: [bool; true] = [];
    let [first, ..] = [1, 2];
}

fn main() {}
