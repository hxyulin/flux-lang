use std::cell::Cell;
std::thread_local!(static FIRST: Cell<u32> = const { Cell::new(40) });
std::thread_local!(static SECOND: Cell<u32> = Cell::new(2));
fn main() {
    let value = FIRST.with(|first| SECOND.with(|second| {
        first.set(first.get() + second.get());
        first.get()
    }));
    assert_eq!(value, 42);
    println!("{value}");
}
