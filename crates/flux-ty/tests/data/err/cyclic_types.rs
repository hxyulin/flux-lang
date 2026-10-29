// A variable cannot be bound to a type that contains it: that type
// would be infinite. rustc reports overflow (E0275); flux reports the
// mismatch, and must not recurse forever resolving the variable.

fn same<T>(_: T, _: T) {}

fn assign() {
    let mut x = None;
    x = Some(x);
}

fn argument() {
    let x = None;
    same(x, Some(x));
}

fn method() {
    let mut v = Vec::new();
    v.push(v);
}

fn main() {}
