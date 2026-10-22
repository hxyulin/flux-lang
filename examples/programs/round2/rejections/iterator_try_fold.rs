//! MISSING FEATURE (documented), REPORTED AS A PLAIN ERROR.
//! `Iterator::try_fold`, here parsing digits with early exit.
//!
//! rustc: compiles; exits with status 42.
//! flux: "no method named `try_fold` found for struct `Bytes` in the
//! current scope" (rustc's E0599 wording on valid code; LANGUAGE.md says
//! `try_fold` is not supported yet), followed by a cascade "the method
//! `is_ascii_digit` on `_` is not supported yet" inside the closure.
//! Blocks round2/bigint.rs, round2/json.rs, round2/brainfuck.rs and
//! round2/inventory.rs.

fn parse(s: &str) -> Option<i32> {
    s.bytes().try_fold(0i32, |n, b| if b.is_ascii_digit() { Some(n * 10 + (b - b'0') as i32) } else { None })
}

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    unsafe { exit(parse("42").unwrap_or(0) + parse("4x").unwrap_or(0)) }
}
