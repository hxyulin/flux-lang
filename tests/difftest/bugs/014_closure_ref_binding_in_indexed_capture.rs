//! Internal compiler errors: a closure matching an element of a captured
//! array with a `ref` binding (`match w[0] { S { a: ref x, b: _ } => *x }`).
//!
//! rustc accepts this and prints `1` and `3`. flux reports "internal
//! compiler error: a local neither declared nor captured" at `v` and `w`,
//! and, for the element of an array inside a tuple, also "internal
//! compiler error: a type that is an error, without a diagnostic" at the
//! `match` (debug and -O alike).
//!
//! Found by difftest (seeds 102041, 102078 and 104282 of seeds
//! 100000..108000, the "type that is an error" ICE; 102041 reduced to a
//! closure matching `v1.1[0]`, then by hand to this).
//!
//! Diagnosis: closure capture analysis for places with an index
//! projection, as in 012: rustc captures `v.1` and `w` by reference
//! (truncated at the index) for the `ref` binding; flux seems to capture
//! nothing for them, then finds the place unbound when building the
//! closure's MIR.

struct S {
    a: u8,
    b: u8,
}

fn main() {
    let v = (1, [S { a: 1, b: 2 }]);
    let c = || match v.1[0] {
        S { a: ref x, b: _ } => *x,
    };
    println!("{}", c());
    let w = [S { a: 3, b: 4 }];
    let d = || match w[0] {
        S { a: ref x, b: _ } => *x,
    };
    println!("{}", d());
}
