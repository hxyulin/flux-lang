// A value whose dereferences reach nothing callable is not a function.
use std::sync::Arc;

fn boxed(b: Box<u32>) -> u32 {
    b(1)
}

fn shared(a: &Arc<String>) {
    a();
}

fn main() {}
