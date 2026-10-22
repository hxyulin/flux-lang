//! An array length written as an expression (an anonymous constant) in the
//! signature of an item a `macro_rules!` macro defines, passed to a const
//! generic function.
//!
//! rustc: compiles; exits with status 4.
//! flux: `flux build` panics in codegen: "internal error: entered
//! unreachable code: instances are of concrete types"
//! (crates/flux-codegen/src/lower.rs, `instance_symbol`): the length seems
//! to reach monomorphization unevaluated. Writing `[u8; 4]` in the macro, or
//! `[u8; 2 + 2]` outside a macro, works. Found in round2/hashing.rs, whose
//! `fnv!` macro writes `type Output = [u8; (<$ty>::BITS / 8) as usize];`.

fn len_of<const N: usize>(_: [u8; N]) -> usize {
    N
}

macro_rules! make {
    () => {
        fn make() -> [u8; 2 + 2] {
            [0; 4]
        }
    };
}

make!();

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    unsafe { exit(len_of(make()) as i32) }
}
