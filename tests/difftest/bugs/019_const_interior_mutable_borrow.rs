//! Manual self-hosting prerequisite probe; no generator seed.
//! Rustc rejects E0492: a constant may not extend the lifetime of a shared
//! borrow of a temporary containing interior mutability. Flux accepts MIR.
//! Reproduced with the pre-type-name release compiler (15e9cb3), so this
//! predates diagnostic type names. Invalid program: compile only, never run.
use std::cell::Cell;

const CELL: &Cell<u8> = &Cell::new(1);

fn main() {
    let _ = CELL;
}
