//! Internal compiler error: a closure matching a captured enum with
//! only one variant (`match v { E::V => 1 }`).
//!
//! rustc accepts this and prints `1`. flux reports "internal compiler
//! error: a pattern reads a place its closure does not capture" at `v`
//! (debug and -O alike).
//!
//! Found by difftest (seeds 101637 and 107014 of seeds 100000..108000;
//! 101637 reduced to a closure in a match guard matching `v1`, then by
//! hand to this).
//!
//! Diagnosis: closure capture analysis (same area as 012). In edition
//! 2021 a pattern that cannot fail, of a single-variant enum without
//! bindings, does not read its scrutinee (rustc does not even read the
//! discriminant of an enum with one variant), so the closure captures
//! nothing; flux's capture analysis agrees, but the match lowering in
//! the closure's MIR still reads the place.

enum E {
    V,
}

fn main() {
    let v = E::V;
    let c = || match v {
        E::V => 1,
    };
    println!("{}", c());
}
