// Const generics: const parameters of integer types, `bool` and `char`
// on structs, `impl` blocks, functions and traits; arguments written as
// literals, paths and other parameters, or inferred; parameters as
// values and array lengths.

const LIMIT: usize = 3;

struct Stack<T, const N: usize> {
    items: [T; N],
    len: usize,
}

impl<T: Copy, const N: usize> Stack<T, N> {
    const CAPACITY: usize = N;

    fn new(fill: T) -> Stack<T, N> {
        Stack {
            items: [fill; N],
            len: 0,
        }
    }

    fn push(&mut self, value: T) -> bool {
        if self.len == N {
            return false;
        }
        self.items[self.len] = value;
        self.len += 1;
        true
    }
}

trait Width {
    const WIDTH: usize;
}

impl<T, const N: usize> Width for [T; N] {
    const WIDTH: usize = N;
}

trait Pick<const I: usize> {
    fn pick(&self) -> i32;
}

struct P;

impl Pick<0> for P {
    fn pick(&self) -> i32 {
        10
    }
}

fn sum<const N: usize>(a: [i32; N]) -> i32 {
    let mut total = 0;
    let mut i = 0;
    while i < N {
        total += a[i];
        i += 1;
    }
    total
}

fn twice<const N: usize>(a: [i32; N]) -> i32 {
    sum::<N>(a) + sum(a)
}

fn offset<const D: i32>(x: i32) -> i32 {
    x + D
}

fn flag<const B: bool, const C: char>() -> u32 {
    if B { C as u32 } else { 0 }
}

fn zeros<const N: usize>() -> [u8; N] {
    let a: [u8; N] = [0; N];
    a
}

fn width<A: Width>(_a: &A) -> usize {
    A::WIDTH
}

fn main() {
    let a = sum([1, 2, 3]);
    let b = twice::<2>([10, 20]);
    let mut s = Stack::<i32, 4>::new(0);
    s.push(5);
    let t: Stack<u8, LIMIT> = Stack::new(1);
    let c = offset::<{ 3 }>(1) + offset::<7>(1);
    let d = flag::<true, 'a'>();
    let z = zeros::<5>();
    let w = width(&z) + Stack::<u8, 2>::CAPACITY;
    let p = <P as Pick<0>>::pick(&P);
}
