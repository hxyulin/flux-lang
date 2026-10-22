// Const generics' errors, as rustc reports them: parameter types,
// argument kinds and counts, mismatched values, uninferred parameters
// and generic arguments in constant expressions.

struct Stack<T, const N: usize> {
    items: [T; N],
    len: usize,
}

const M: u8 = 3;

fn depends<T, const N: T>() {}

fn text<const S: &'static str>() {}

fn float<const F: f32>() {}

fn sum<const N: usize>(a: [i32; N]) -> i32 {
    a[0]
}

fn make<const N: usize>() -> [u8; N] {
    [0; N]
}

fn byte<const B: u8>() -> u8 {
    B
}

fn size<const N: usize>() -> usize {
    N
}

fn generic_len<const N: usize>(x: [u8; N]) -> [u8; 3] {
    x
}

fn as_type<const N: usize>(x: N) {}

fn takes(s: Stack<i32, 3>) {}

fn main() {
    let x: [i32; 3] = make::<4>();
    let s: Stack<i32> = loop {};
    let t: Stack<i32, 3, 4> = loop {};
    let u: Stack<3, i32> = loop {};
    sum::<i32>([1]);
    let y: [u8; 3] = [1, 2, 3, 4];
    let v = Stack { items: [1, 2, 3, 4], len: 0 };
    takes(v);
    let b = size::<true>();
    let c = size::<M>();
    let e = size::<'x'>();
    let f: [u8; M] = [];
    let g = sum::<1, 2>([1]);
}

fn uninferred() {
    let a = make();
}
