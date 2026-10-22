// Borrow checking of trait objects: a value made a trait object must
// outlive the object's lifetime bound, which is its reference's, as
// rustc's default object lifetimes are.

trait Tr {
    fn get(&self) -> i32;
}
struct S<'a>(&'a i32);
impl Tr for S<'_> {
    fn get(&self) -> i32 {
        *self.0
    }
}

fn keep<'a>(s: &'a S<'_>) -> &'a dyn Tr {
    s
}

fn too_long<'a, 'b>(s: &'a S<'b>) -> &'static dyn Tr {
    s
}

fn param<'a, T: Tr>(x: &'a T) -> &'static dyn Tr {
    x
}

fn dangling() {
    let r: &dyn Tr;
    {
        let x = 5;
        let s = S(&x);
        r = &s;
    }
    r.get();
}

fn overwritten() {
    let mut x = 5;
    let s = S(&x);
    let r: &dyn Tr = &s;
    x = 6;
    r.get();
}

fn main() {}

// A trait object's lifetime bound cannot be checked against an `impl`
// block's parameter's: rustc's E0521 here, not supported yet in flux.
trait Two {
    fn two(&self) {}
}
impl<T: ?Sized + 'static> Two for T {}
fn escapes_through_impl<'a>(x: &'a dyn Tr) {
    x.two();
}
