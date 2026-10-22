// Defaults of structs' and enums' type parameters, in signatures and
// annotations, with lifetimes before them, after them in terms of the
// ones before, and for every parameter (`D` is `D<u8>`).

use core::ops::ControlFlow;

struct W<'a, T = i32>(&'a T, u8);

struct D<T = u8>(T);

enum Pick<A, B = A> {
    L(A),
    R(B),
}

fn get<'a>(w: W<'a>) -> &'a i32 {
    w.0
}

fn second<'a>(w: &W<'a, u8>) -> &'a u8 {
    w.0
}

fn pick(p: Pick<i32>) -> i32 {
    match p {
        Pick::L(a) => a,
        Pick::R(b) => b * 2,
    }
}

fn stop(n: i32) -> ControlFlow<i32> {
    if n > 3 { ControlFlow::Break(n) } else { ControlFlow::Continue(()) }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let x = 5;
    let y = 7u8;
    let d: D = D(3);
    let w = W(&y, 1);
    let total = *get(W(&x, 0)) + *second(&w) as i32 + d.0 as i32 + pick(Pick::R(4));
    let c: ControlFlow<i32> = stop(total);
    let code = match c {
        ControlFlow::Break(n) => n,
        ControlFlow::Continue(()) => 0,
    };
    unsafe { exit(code) }
}
