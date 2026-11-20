use std::cell::Cell;
use std::num::Saturating;
use std::sync::{Arc, OnceLock};

thread_local! { static COUNTER: Cell<u8> = Cell::new(2); }

fn fail<T>() -> Result<T, u8> { Err(6) }
fn run() -> Result<(), u8> {
    fail()?;
    Ok(())
}

#[allow(unreachable_code)]
fn check_diverging_binding() {
    let value = panic!("unreachable");
    let _: &u32 = &value;
}

fn main() {
    let mut value = Saturating(254u8);
    value += 3u8;
    let cell = OnceLock::from(value.0);
    println!("{value:?} {:?} {}", cell.get(), COUNTER.replace(4));
    println!("{} {:?}", COUNTER.get(), run());
    let owner = Arc::new(5);
    let weak = Arc::downgrade(&owner);
    println!("{}", weak.strong_count());
    drop(owner);
    println!("{}", weak.strong_count());
}
