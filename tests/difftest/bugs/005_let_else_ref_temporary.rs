//! Wrong rejection: a `ref` binding in `let ... else` of a temporary
//! does not extend the temporary's lifetime.
//!
//! rustc accepts this (the temporary `[3u8; 3]` lives to the end of the
//! block, as for `let ref b = [3u8; 3];`) and exits with 3. flux reports
//! "temporary value dropped while borrowed" at the array (debug and -O
//! alike).
//!
//! Found by difftest (seed 111 of the first campaign; `--weight
//! ref_let_else=0` keeps `ref` out of `let ... else` patterns).
//!
//! Diagnosis: temporary lifetime extension applies to `let` with a `ref`
//! pattern but not to `let ... else`, which rustc extends the same way.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn f() -> Option<u8> {
    let ref b = [3u8; 3] else {
        return None;
    };
    Some(b[1])
}

fn main() {
    unsafe { exit(f().unwrap() as i32) }
}
