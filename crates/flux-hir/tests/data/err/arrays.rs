// Array lengths that name locals or generic parameters, and slice
// patterns.

const N: usize = 2;

struct W<T>(T);

impl<T> W<T> {
    const LEN: usize = 2;

    fn of_self() -> [u8; Self::LEN + 1] {
        loop {}
    }
}

trait Tr {
    fn of_self() -> [u8; Self::N * 2];
}

fn lengths(n: usize) {
    let a = [0; n];
    let b: [u8; N + 1] = [0; 3];
    let c = [0; { 3 }];
    let d: [bool; true] = [];
    let [first, ..] = [1, 2];
    let e = [0; n + 1];
    let f: [u8; n] = [0; 2];
    let g: [u8; { n }] = [0; 2];
}

fn generic<T, const M: usize>() -> [T; M + 1] {
    let h = [0u8; M * 2];
    let i: [u8; size_of::<T>()] = loop {};
    let j = [0u8; size_of::<T>()];
    loop {}
}

fn main() {}
