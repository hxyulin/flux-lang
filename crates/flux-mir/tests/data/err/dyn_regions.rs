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

// A trait object's lifetime bound against a parameter's (E0521).
fn need_static<T: ?Sized + 'static>(_: &T) {}
fn escapes<'a>(x: &'a dyn Tr) {
    need_static(x);
}
fn still_static(x: &(dyn Tr + 'static)) {
    need_static(x);
}

// Lifetime bounds written out, which may only shorten.
fn param_bound<'a, T: Tr>(x: &'a T) -> &'a (dyn Tr + 'static) {
    x
}
fn too_short<'a, 'b>(x: &'a (dyn Tr + 'b)) -> &'a (dyn Tr + 'static) {
    x
}
fn shorter<'a>(x: &'a (dyn Tr + 'static)) -> &'a dyn Tr {
    x
}
fn reborrowed<'a>(x: &'a mut (dyn Tr + 'static)) -> &'a mut dyn Tr {
    x
}

trait Two {
    fn two(&self) {}
}
impl<T: ?Sized + 'static> Two for T {}
fn escapes_through_impl<'a>(x: &'a dyn Tr) {
    x.two();
}
