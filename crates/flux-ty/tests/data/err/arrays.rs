// Type errors of arrays and slices, as rustc reports them.

struct S;

const N: usize = 3;
const M: u8 = 2;

fn sizes() {
    let a: [i32; N] = [1, 2];
    let b: [u8; 2] = [1i32, 2];
    let c = [1, true];
    let d: [u8; 3u8] = [1, 2, 3];
    let e: [u8; M] = [1, 2];
}

fn indices(a: [i32; 3], s: &[i32]) {
    let i: i32 = 1;
    let x = a[i];
    let y = s[i];
    let n = 5;
    let z = n[0];
    let w = (1, 2)[0];
}

fn copies<T>(t: T) -> [T; 2] {
    let one = [S; 1];
    let two = [S; 2];
    [t; 2]
}

fn unsized_values(s: &[i32]) {
    let u = *s;
    let q: [i32] = [1];
    let m: &mut [i32] = &[1, 2];
}

fn unsized_param(x: [i32]) {}

fn unsized_return() -> [i32] {
    loop {}
}

fn unsized_arg(x: Option<[i32]>) {}

fn generic<T>(x: &T) -> usize {
    0
}

fn unsized_generic(s: &[i32]) -> usize {
    generic(s)
}

fn main() {}
