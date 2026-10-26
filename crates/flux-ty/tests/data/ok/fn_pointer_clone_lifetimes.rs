fn duplicate<T: Clone>(value: &T) -> T {
    value.clone()
}

// Clone preserves a function pointer's represented lifetime contract.
fn output(pointer: fn(&i32) -> &i32) {
    let _ = duplicate(&pointer);
}
fn named(pointer: fn(&'static i32) -> i32) {
    let _ = duplicate(&pointer);
}
fn main() {}
