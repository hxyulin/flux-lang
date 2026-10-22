//! MISSING FEATURES. `str` methods and patterns flux's `core` lacks:
//! - `strip_prefix`, `strip_suffix`, `split_once`: "the method
//!   `strip_prefix` on `&str` is not supported yet", with cascades such as
//!   "the method `trim` on `_` is not supported yet" and "type annotations
//!   needed" on the results;
//! - `rfind`: "no method named `rfind` found for reference `&str` in the
//!   current scope" (rustc's E0599 wording on valid code);
//! - a closure as a pattern (`s.find(|c| c == ';' || c == '#')`): "the
//!   trait bound `{closure}: Pattern` is not satisfied" (a plain E0277 on
//!   valid code);
//! - `bytes().rposition(..)`: "the trait bound `Bytes: ExactSizeIterator`
//!   is not satisfied" (rustc's `Bytes` is `ExactSizeIterator` and
//!   `DoubleEndedIterator`).
//!
//! rustc: compiles; exits with status 28.
//! Block round2/ini_csv.rs, round2/inventory.rs and round2/json.rs.

unsafe extern "C" {
    fn exit(code: i32) -> !;
}

fn main() {
    let line = "[section] ; comment";
    let body = line.strip_prefix('[').and_then(|s| s.split_once(']')).map_or("", |(name, _)| name);
    let comment = line.find(|c| c == ';' || c == '#').unwrap_or(0);
    let last_space = line.rfind(' ').unwrap_or(0);
    let last_s = line.bytes().rposition(|b| b == b's').unwrap_or(0);
    let tail = "a.rs".strip_suffix(".rs").map_or(0, |s| s.len());
    unsafe { exit((body.len() + comment + last_space - last_s + tail) as i32) }
}
