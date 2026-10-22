//! Internal compiler error: a closure that matches an element of a
//! captured array with only a wildcard (`match p[0] { _ => {} }`).
//!
//! rustc accepts this and prints `1 2`. flux reports "internal compiler
//! error: a local neither declared nor captured" at `p` in `indexed`
//! (debug and -O alike); `field`, matching a captured tuple's field the
//! same way, is fine.
//!
//! Found by difftest: the most common rejection in seeds
//! 100000..108000 (33 programs, first 100384, reduced from 13 KB).
//!
//! Diagnosis: in edition 2021, a `match` whose arms only have wildcards
//! does not read its scrutinee, so the closure captures nothing for it
//! (rustc records a fake read only, and the index expression `0` is
//! still evaluated). flux's capture analysis seems to skip the place,
//! while MIR building of the closure body still lowers the indexed
//! scrutinee and finds `p` neither a local nor a capture: for an index
//! projection the place must still be captured (rustc truncates the
//! capture at the index: `p` is captured by reference as the index is
//! evaluated), or the scrutinee not lowered.

fn indexed(p: [u8; 1]) -> u8 {
    let c = || {
        match p[0] {
            _ => {}
        }
        1
    };
    c()
}

fn field(p: (u8, u8)) -> u8 {
    let c = || {
        match p.1 {
            _ => {}
        }
        2
    };
    c()
}

fn main() {
    println!("{} {}", indexed([7]), field((1, 2)));
}
