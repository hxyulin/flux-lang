// Errors in trait object types and their uses, as rustc reports them.

trait Tr {
    fn get(&self) -> i32;
    fn by(self);
    fn sized(&self)
    where
        Self: Sized,
    {
    }
}
impl Tr for i32 {
    fn get(&self) -> i32 {
        *self
    }
    fn by(self) {}
}
impl Tr for str {
    fn get(&self) -> i32 {
        0
    }
    fn by(self) {}
}
trait Other {}
trait Two {
    type X;
    type Y;
}
trait Add2<Rhs = Self> {
    fn add(&self, r: &Rhs);
}
trait It {
    type Item;
    fn next(&mut self) -> Option<Self::Item>;
}
struct C;
impl It for C {
    type Item = u8;
    fn next(&mut self) -> Option<u8> {
        None
    }
}

fn defaults(_: &dyn Add2) {}
fn missing(_: &dyn Two) {}
fn missing_one(_: &dyn Two<X = u8>) {}
fn extra(_: &(dyn Tr + Other)) {}
fn bare(_: &Tr) {}

fn not_impl(x: &u8) -> &dyn Tr {
    x
}
fn unsized_value(x: &str) -> &dyn Tr {
    x
}
fn not_super(x: &dyn Tr) -> &dyn Other {
    x
}
fn wrong_item(x: &mut C) -> &mut dyn It<Item = u16> {
    x
}
fn not_mut(x: &i32) -> &mut dyn Tr {
    x
}
fn maybe_unsized<T: ?Sized + Tr>(x: &T) -> &dyn Tr {
    x
}
fn calls(k: &dyn Tr) {
    k.sized();
    k.nope();
}

// rustc reports this after type checking (E0161, with E0507).
fn by_value(k: &dyn Tr) {
    k.by();
}

fn main() {}
