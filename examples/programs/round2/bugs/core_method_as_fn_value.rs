//! A `core` method named as a function value (not called) and passed to an
//! iterator adapter: `iter.all(u8::is_ascii_hexdigit)`.
//!
//! rustc: compiles; exits with status 1.
//! flux: type-checks, then fails at build: "internal error: flux produced
//! invalid keel IR: @..FnMut8call_mut..: calls
//! @_ZN4core5ascii2u817is_ascii_hexdigitE, which is not declared". The
//! `core` function is never instantiated when it is only reached as a value
//! (core's bodies are compiled where the program reaches them, and this
//! path seems to be missed). The same happens for a generic one,
//! `lexer.filter(Result::is_ok)` (`core::result::Result::is_ok`), in
//! round2/json.rs. A closure `|b| b.is_ascii_hexdigit()` works.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let all_hex = b"1f2a".iter().all(u8::is_ascii_hexdigit);
    unsafe { exit(all_hex as i32) }
}
