//! `#[track_caller]`: a function marked with it reports where it was
//! called, through other marked functions, in `Location::caller()` and
//! in the panics of `unwrap` and friends it calls.

use core::panic::Location;

#[track_caller]
fn here() -> &'static Location<'static> {
    Location::caller()
}

#[track_caller]
fn outer() -> &'static Location<'static> {
    here()
}

fn not_tracked() -> &'static Location<'static> {
    here()
}

#[track_caller]
fn first(values: &[Option<u32>]) -> u32 {
    values[0].unwrap() + checked(values)
}

#[track_caller]
fn checked(values: &[Option<u32>]) -> u32 {
    values[1].expect("the second value")
}

fn main() {
    let l = here();
    println!("{}:{}:{}", l.file(), l.line(), l.column());
    println!("{}", outer());
    println!("{}", not_tracked());
    println!("{:?}", Location::caller().line());
    let values = [Some(1), Some(2)];
    println!("{}", first(&values));
    let values = [Some(1), None];
    println!("{}", first(
        &values,
    ));
}
