//! MISSING FEATURE (documented). Array and slice patterns: destructuring a
//! state array (`let [a, b, c, d] = state;`) and matching a byte slice's
//! sign (`[b'-', rest @ ..]`).
//!
//! rustc: compiles; exits with status 13.
//! flux: "slice patterns are not supported yet", followed by a cascade of
//! "cannot find value `rest` in this scope" (and `a`, `b`, ... in
//! round2/hashing.rs, 34 of them): the bindings of the rejected pattern
//! are dropped instead of being declared with an error type.
//! Blocks round2/hashing.rs and round2/ini_csv.rs.

fn sign(s: &[u8]) -> (bool, &[u8]) {
    match s {
        [b'-', rest @ ..] => (true, rest),
        all => (false, all),
    }
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let [a, b, c, d] = [1, 2, 3, 4];
    let (neg, rest) = sign(b"-12");
    unsafe { exit(a + b + c + d + neg as i32 + rest.len() as i32) }
}
