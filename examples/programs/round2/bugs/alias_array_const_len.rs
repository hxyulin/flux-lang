//! A type alias of an array whose length is a named constant.
//!
//! rustc: compiles; exits with status 4.
//! flux: no diagnostic, but the alias's type is `{error}` (see `flux mir`:
//! `let _1: {error}; // p`), and `flux build` panics in keel's frontend
//! ("var1 has type i64, assigned a i32"). With `type Pair = [u32; 2];`, or
//! with `[u32; N]` written out instead of the alias, it works.
//! Variants seen in round2/graphs.rs: `type Matrix = [[u32; V]; V]` as a
//! parameter type gives "the method `iter` on `_` is not supported yet" and
//! "the type `[u32]` cannot be indexed by `usize`"; as `[u32; V]` in a
//! function signature, keel IR verification fails ("takes (), got (ptr)") or
//! codegen panics at lower.rs ("index out of bounds: the len is 0").

const N: usize = 2;
type Pair = [u32; N];

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let p: Pair = [3, 4];
    unsafe { exit(p[1] as i32) }
}
