use std::cell::Cell;

struct Guard;
impl Drop for Guard {
    fn drop(&mut self) {
        assert!(GUARD.try_with(|_| ()).is_err());
        let value = VALUE.try_with(|cell| cell.get()).unwrap_or(999);
        assert_eq!(value, 19);
        println!("value {}", value);
    }
}
thread_local! {
    static GUARD: Guard = Guard;
    static VALUE: Cell<u32> = const { Cell::new(0) };
}
fn main() {
    GUARD.with(|_| ());
    VALUE.set(19);
}
