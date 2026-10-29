use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, RefUnwindSafe, UnwindSafe};

fn run<F: FnOnce() -> u32 + UnwindSafe>(f: F) -> u32 {
    f()
}
fn shared<T: RefUnwindSafe>(value: &T) -> &T {
    value
}

fn main() {
    let mut value = 7;
    let result = run(AssertUnwindSafe(|| {
        value += 1;
        value
    }));
    let cell = Cell::new(11);
    let owned = run(move || cell.get());
    let values = vec![RefCell::new(13)];
    let vector = run(move || *values[0].borrow());
    let lock = std::sync::Mutex::new(Cell::new(17));
    let locked = shared(&lock).lock().unwrap().get();
    println!("{result} {owned} {vector} {locked} {value}");
}
