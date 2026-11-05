// Receivers that rustc rejects, and a trait whose receiver a trait
// object cannot dispatch on.
use std::rc::Rc;
use std::sync::Arc;

struct S;

impl S {
    fn optional(self: Option<Self>) {}
    fn nested(self: Box<Rc<Self>>) {}
}

trait Shared {
    fn by_ref(self: &Arc<Self>);
}

fn object(_: &dyn Shared) {}

trait Consume {
    fn consume(self);
}

fn moved(b: Box<dyn Consume>) {
    b.consume();
}

fn main() {}
