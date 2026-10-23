fn duplicate<T: Clone>(value: &T) -> T {
    value.clone()
}

// Clone imposes no bound on the signature's types, but cannot make
// unsupported pointer lifetime annotations representable.
fn output(pointer: fn(&i32) -> &i32) {
    let _ = duplicate(&pointer);
}
fn named(pointer: fn(&'static i32) -> i32) {
    let _ = duplicate(&pointer);
}
fn main() {}
