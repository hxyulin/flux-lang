// Const parameters where rustc rejects them before type checking:
// generic arguments in constant expressions, in patterns, and shadowed
// by bindings.

fn grown<const N: usize>() -> [u8; N + 1] {
    loop {}
}

fn doubled<const N: usize>() -> usize {
    let a = [0u8; N * 2];
    a.len()
}

fn in_pattern<const N: usize>(x: usize) -> bool {
    match x {
        N => true,
        _ => false,
    }
}

fn shadowed<const N: usize>() {
    let mut N = 1;
}

fn arg<const N: usize>() -> usize {
    N
}

fn call<const N: usize>() -> usize {
    arg::<{ N + 1 }>()
}

fn main() {}
